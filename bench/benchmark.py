#!/usr/bin/env python3
"""Benchmark Lucent against mako, dunst, swaync and fnott.

One command prints a Markdown table ready for the README:

    python3 bench/benchmark.py > results.md

Every daemon runs on its own private dbus-daemon, started and stopped by this
script, so the desktop's own notification daemon is never stopped, replaced or
even contacted. Popups still appear on the real compositor, because drawing
them is what is being measured. See bench/README.md for the method.

Standard library only; needs dbus-daemon, notify-send and gdbus on PATH.
"""

import argparse
import json
import os
import platform
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# --------------------------------------------------------------------------
# Pure parts (tested in test_benchmark.py)
# --------------------------------------------------------------------------

WRAP_US = 2**32
_LINE = re.compile(
    r"^\[\s*(\d+)\.(\d{3})\](?:\[rs\])?\s*->\s*"
    r"wl_surface[@#](\d+)\.(attach|commit)\((.*)\)"
)


def wl_stamp_us(realtime_ns):
    """A wall-clock instant as WAYLAND_DEBUG stamps it: µs since epoch mod 2^32.

    libwayland and wayland-rs both print this, as `[ms.µs]`."""
    return (realtime_ns // 1000) % WRAP_US


def first_frame_ms(log, t0_us):
    """Milliseconds from `t0_us` (a WAYLAND_DEBUG-style stamp) to the first
    wl_surface.commit that carries a buffer attached after t0, or None."""
    pending = set()
    for line in log.splitlines():
        m = _LINE.match(line)
        if not m:
            continue
        stamp = int(m[1]) * 1000 + int(m[2])
        delta = (stamp - t0_us) % WRAP_US
        if delta > WRAP_US // 2:  # before t0
            continue
        surface, request, args = m[3], m[4], m[5]
        if request == "attach":
            if args.startswith(("nil", "None")):
                pending.discard(surface)
            else:
                pending.add(surface)
        elif surface in pending:
            return delta / 1000
    return None


def _status_field(status, name):
    for line in status.splitlines():
        key, _, value = line.partition(":")
        if key == name:
            return value.split()[0]
    return None


def context_switches(statuses):
    """Voluntary plus involuntary context switches summed over the
    /proc/<pid>/task/*/status texts of one process: its wakeups."""
    return sum(
        int(_status_field(s, f) or 0)
        for s in statuses
        for f in ("voluntary_ctxt_switches", "nonvoluntary_ctxt_switches")
    )


def status_kib(status, name):
    """A `kB` field of a /proc status or smaps_rollup text, in bytes, or None."""
    value = _status_field(status, name)
    return None if value is None else int(value) * 1024


@dataclass
class Result:
    daemon: str
    version: str
    binary_bytes: int
    idle_rss: int | None  # None: the daemon was not running at all
    idle_wakeups: int
    peak_rss: int | None
    popups_pss: int | None
    cold_ms: list = field(default_factory=list)
    warm_ms: list = field(default_factory=list)


def _size(n):
    if n is None:
        return "n/a"
    if n < 1024 * 1024:
        return f"{n / 1024:.0f} KiB"
    return f"{n / (1024 * 1024):.1f} MiB"


def _latency(samples):
    return f"{statistics.median(samples):.1f} ms" if samples else "n/a"


def markdown_table(rows):
    out = [
        "| Daemon | Version | Binary size | Idle RSS | Idle wakeups "
        "| Peak RSS, 5 Popups | PSS, 5 Popups | Cold start | Warm |",
        "|---|---|--:|--:|--:|--:|--:|--:|--:|",
    ]
    for r in rows:
        idle = "0 (not running)" if r.idle_rss is None else _size(r.idle_rss)
        out.append(
            f"| {r.daemon} | {r.version} | {_size(r.binary_bytes)} | {idle} "
            f"| {r.idle_wakeups} | {_size(r.peak_rss)} | {_size(r.popups_pss)} "
            f"| {_latency(r.cold_ms)} | {_latency(r.warm_ms)} |"
        )
    return "\n".join(out) + "\n"


# --------------------------------------------------------------------------
# Daemons
# --------------------------------------------------------------------------


@dataclass
class Daemon:
    name: str
    binary: str
    args: list = field(default_factory=list)
    # Files written under the daemon's private XDG_CONFIG_HOME.
    config: dict = field(default_factory=dict)


def find_lucent(explicit):
    for candidate in (explicit, shutil.which("lucent"), REPO / "target/release/lucent"):
        if candidate and Path(candidate).is_file():
            return str(Path(candidate).resolve())
    return None


def daemons(lucent):
    found = []
    if lucent:
        found.append(Daemon("lucent", lucent))
        found.append(
            Daemon(
                "lucent (resident)",
                lucent,
                config={"lucent/config.toml": "[general]\nidle_exit = false\n"},
            )
        )
    for name in ("mako", "dunst", "swaync", "fnott"):
        path = shutil.which(name)
        if path:
            found.append(Daemon(name, str(Path(path).resolve())))
        else:
            log(f"{name}: not installed, skipped")
    return found


def version(binary):
    try:
        out = subprocess.run(
            ["pacman", "-Qo", binary], capture_output=True, text=True, check=True
        ).stdout.split()
        return out[-1]  # "<path> is owned by <pkg> <version>"
    except (OSError, subprocess.CalledProcessError, IndexError):
        pass
    if Path(binary).is_relative_to(REPO):
        cargo = (REPO / "Cargo.toml").read_text()
        ver = re.search(r'^version\s*=\s*"([^"]+)"', cargo, re.M)[1]
        try:
            rev = subprocess.run(
                ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"],
                capture_output=True, text=True, check=True,
            ).stdout.strip()
            return f"{ver} ({rev})"
        except (OSError, subprocess.CalledProcessError):
            return ver
    return "unpackaged"


# --------------------------------------------------------------------------
# A private session bus with one activatable daemon
# --------------------------------------------------------------------------

BUS_CONF = """\
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <keep_umask/>
  <listen>unix:path={socket}</listen>
  <servicedir>{services}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"""


class Bus:
    """A throwaway dbus-daemon on which `daemon` is D-Bus activatable as
    org.freedesktop.Notifications, with its own config and runtime dirs."""

    def __init__(self, daemon, root, wayland_debug):
        self.root = Path(tempfile.mkdtemp(dir=root, prefix="bus-"))
        self.log = self.root / "daemon.log"
        self.pids = self.root / "pids"
        # Socket paths are capped at 108 bytes, so the socket lives in a short
        # directory under the real runtime dir rather than next to the logs.
        self.sock_dir = Path(
            tempfile.mkdtemp(dir=os.environ["XDG_RUNTIME_DIR"], prefix="lucent-bench-")
        )
        socket = self.sock_dir / "bus"
        services = self.root / "services"
        services.mkdir()
        config_home = self.root / "config"
        runtime = self.root / "runtime"
        runtime.mkdir(mode=0o700)
        for rel, text in daemon.config.items():
            (config_home / rel).parent.mkdir(parents=True, exist_ok=True)
            (config_home / rel).write_text(text)
        config_home.mkdir(exist_ok=True)

        real_runtime = os.environ["XDG_RUNTIME_DIR"]
        display = os.environ.get("WAYLAND_DISPLAY", "wayland-0")
        self.env = {
            "PATH": os.environ.get("PATH", "/usr/bin"),
            "HOME": os.environ.get("HOME", "/"),
            "LANG": os.environ.get("LANG", "C.UTF-8"),
            "XDG_CONFIG_HOME": str(config_home),
            "XDG_RUNTIME_DIR": str(runtime),
            "WAYLAND_DISPLAY": os.path.join(real_runtime, display),
            "DBUS_SESSION_BUS_ADDRESS": f"unix:path={socket}",
        }
        if wayland_debug:
            self.env["WAYLAND_DEBUG"] = "1"

        wrapper = self.root / "start"
        argv = " ".join(_sh_quote(a) for a in [daemon.binary, *daemon.args])
        wrapper.write_text(
            "#!/bin/sh\n"
            f"echo $$ >> {_sh_quote(str(self.pids))}\n"
            f"exec {argv} >> {_sh_quote(str(self.log))} 2>&1\n"
        )
        wrapper.chmod(0o755)
        (services / "org.freedesktop.Notifications.service").write_text(
            "[D-BUS Service]\nName=org.freedesktop.Notifications\n"
            f"Exec={wrapper}\n"
        )
        conf = self.root / "bus.conf"
        conf.write_text(BUS_CONF.format(socket=socket, services=services))
        self.proc = subprocess.Popen(
            ["dbus-daemon", "--nofork", "--nopidfile", f"--config-file={conf}"],
            env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        deadline = time.monotonic() + 5
        while not socket.exists():
            if time.monotonic() > deadline or self.proc.poll() is not None:
                raise RuntimeError("private dbus-daemon did not start")
            time.sleep(0.01)

    def run(self, *argv):
        return subprocess.run(
            argv, env=self.env, check=True, capture_output=True, timeout=20
        )

    def notify(self, summary, timeout_ms=5000):
        self.run("notify-send", "-t", str(timeout_ms), summary, "Lucent benchmark body")

    def server_information(self):
        self.run(
            "gdbus", "call", "--session",
            "--dest", "org.freedesktop.Notifications",
            "--object-path", "/org/freedesktop/Notifications",
            "--method", "org.freedesktop.Notifications.GetServerInformation",
        )

    def pid(self):
        """The most recently activated daemon process, if it is still alive."""
        try:
            pid = int(self.pids.read_text().split()[-1])
        except (OSError, IndexError, ValueError):
            return None
        return pid if Path(f"/proc/{pid}").exists() else None

    def read_log(self):
        try:
            return self.log.read_text(errors="replace")
        except OSError:
            return ""

    def close(self):
        try:
            pids = [int(p) for p in self.pids.read_text().split()]
        except OSError:
            pids = []
        for pid in pids:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        self.proc.terminate()
        self.proc.wait(timeout=5)
        deadline = time.monotonic() + 3
        for pid in pids:
            while Path(f"/proc/{pid}").exists() and time.monotonic() < deadline:
                time.sleep(0.02)
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        shutil.rmtree(self.sock_dir, ignore_errors=True)


def _sh_quote(s):
    return "'" + s.replace("'", "'\\''") + "'"


def log(msg):
    print(msg, file=sys.stderr, flush=True)


# --------------------------------------------------------------------------
# Measurements
# --------------------------------------------------------------------------


def proc_statuses(pid):
    texts = []
    for task in Path(f"/proc/{pid}/task").glob("*"):
        try:
            texts.append((task / "status").read_text())
        except OSError:
            pass
    return texts


def proc_status(pid):
    try:
        return Path(f"/proc/{pid}/status").read_text()
    except OSError:
        return ""


def wait_frame(bus, t0_us, timeout=10.0):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        ms = first_frame_ms(bus.read_log(), t0_us)
        if ms is not None:
            return ms
        time.sleep(0.05)
    return None


def measure_cold(daemon, root, runs):
    """notify-send to first frame, with no daemon process running."""
    samples = []
    for i in range(runs + 1):  # the first run only warms the page cache
        bus = Bus(daemon, root, wayland_debug=True)
        try:
            t0 = wl_stamp_us(time.time_ns())
            bus.notify(f"Cold start {i}")
            ms = wait_frame(bus, t0)
        finally:
            bus.close()
        if ms is None:
            log(f"  cold run {i}: no frame seen")
        elif i > 0:
            samples.append(ms)
    return samples


def measure_warm(daemon, root, runs):
    """notify-send to first frame, with the daemon already up and idle."""
    samples = []
    bus = Bus(daemon, root, wayland_debug=True)
    try:
        for i in range(runs):
            # Start (or keep alive) the daemon, let that Popup expire, then
            # measure well inside Lucent's 5 s idle grace.
            bus.notify("Warm-up", timeout_ms=500)
            time.sleep(1.5)
            t0 = wl_stamp_us(time.time_ns())
            bus.notify(f"Warm {i}", timeout_ms=500)
            ms = wait_frame(bus, t0)
            if ms is None:
                log(f"  warm run {i}: no frame seen")
            else:
                samples.append(ms)
    finally:
        bus.close()
    return samples


def measure_idle(daemon, root, settle, window):
    """RSS at the end of, and wakeups during, `window` seconds of idling,
    starting `settle` seconds after the daemon is activated without showing
    anything."""
    bus = Bus(daemon, root, wayland_debug=False)
    try:
        bus.server_information()
        pid = bus.pid()
        time.sleep(settle)
        first = last = None
        end = time.monotonic() + window
        while True:
            if pid and Path(f"/proc/{pid}").exists():
                count = context_switches(proc_statuses(pid))
                first = count if first is None else first
                last = count
            if time.monotonic() >= end:
                break
            time.sleep(1)
        alive = pid is not None and Path(f"/proc/{pid}").exists()
        rss = status_kib(proc_status(pid), "VmRSS") if alive else None
        wakeups = 0 if first is None else last - first
        return rss, wakeups
    finally:
        bus.close()


def measure_peak(daemon, root):
    """Peak RSS (VmHWM) and current PSS of a fresh daemon showing 5 Popups."""
    bus = Bus(daemon, root, wayland_debug=False)
    try:
        for i in range(5):
            bus.notify(f"Popup {i + 1} of 5", timeout_ms=15000)
        time.sleep(2)
        pid = bus.pid()
        if not pid:
            return None, None
        try:
            rollup = Path(f"/proc/{pid}/smaps_rollup").read_text()
        except OSError:
            rollup = ""
        return status_kib(proc_status(pid), "VmHWM"), status_kib(rollup, "Pss")
    finally:
        bus.close()


def machine():
    cpu = "unknown CPU"
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                cpu = line.split(":", 1)[1].strip()
                break
    except OSError:
        pass
    try:
        kib = int(Path("/proc/meminfo").read_text().split()[1])
        mem = f"{kib / 1024 / 1024:.0f} GiB RAM"
    except (OSError, ValueError, IndexError):
        mem = "unknown RAM"
    desktop = os.environ.get("XDG_CURRENT_DESKTOP", "unknown compositor")
    return f"{cpu}, {mem}, Linux {platform.release()}, {desktop}"


def main():
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--lucent", help="Lucent binary (default: PATH, then target/release)")
    p.add_argument("--only", help="comma-separated daemon names to run")
    p.add_argument("--runs", type=int, default=10, help="latency samples per daemon")
    p.add_argument("--idle-seconds", type=float, default=60, help="idle window")
    p.add_argument("--settle", type=float, default=10, help="wait before the idle window")
    p.add_argument("--out", help="directory for raw JSON and daemon logs")
    a = p.parse_args()

    for tool in ("dbus-daemon", "notify-send", "gdbus"):
        if not shutil.which(tool):
            sys.exit(f"{tool} is required")
    if "XDG_RUNTIME_DIR" not in os.environ or "WAYLAND_DISPLAY" not in os.environ:
        sys.exit("run this inside a Wayland session")

    lucent = find_lucent(a.lucent)
    if not lucent:
        log("lucent: no binary found; build one with `cargo build --release`")
    chosen = daemons(lucent)
    if a.only:
        wanted = set(a.only.split(","))
        chosen = [d for d in chosen if d.name in wanted]
    if not chosen:
        sys.exit("no daemon to measure")

    root = Path(a.out or tempfile.mkdtemp(prefix="lucent-bench-"))
    root.mkdir(parents=True, exist_ok=True)
    log(f"raw results and logs: {root}")

    results = []
    for d in chosen:
        log(f"{d.name}: {d.binary}")
        droot = root / re.sub(r"\W+", "-", d.name).strip("-")
        droot.mkdir(exist_ok=True)
        log("  cold start")
        cold = measure_cold(d, droot, a.runs)
        log("  warm")
        warm = measure_warm(d, droot, a.runs)
        log("  peak memory")
        peak, pss = measure_peak(d, droot)
        log(f"  idle ({a.settle:.0f} s settle + {a.idle_seconds:.0f} s window)")
        idle_rss, wakeups = measure_idle(d, droot, a.settle, a.idle_seconds)
        results.append(
            Result(
                daemon=d.name,
                version=version(d.binary),
                binary_bytes=Path(d.binary).stat().st_size,
                idle_rss=idle_rss,
                idle_wakeups=wakeups,
                peak_rss=peak,
                popups_pss=pss,
                cold_ms=cold,
                warm_ms=warm,
            )
        )

    stamp = time.strftime("%Y-%m-%d")
    report = (
        f"Measured {stamp} on {machine()} with `bench/benchmark.py "
        f"--runs {a.runs} --idle-seconds {a.idle_seconds:g}`; latencies are "
        f"medians.\n\n" + markdown_table(results)
    )
    (root / "results.json").write_text(
        json.dumps({"machine": machine(), "results": [asdict(r) for r in results]}, indent=2)
    )
    (root / "results.md").write_text(report)
    print(report, end="")


if __name__ == "__main__":
    main()
