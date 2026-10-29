# Benchmark

`benchmark.py` backs up the claim in Lucent's description: it measures Lucent
next to mako, dunst, swaync and fnott (whichever are installed) and prints a
Markdown table ready to paste into the README. Run it by hand before each
release; it needs a live Wayland session, so it does not run in CI.

```sh
cargo build --release
python3 bench/benchmark.py > results.md
```

It takes about two minutes per daemon. Popups flash on screen while it runs:
drawing them is part of what is measured. Raw samples (`results.json`), the
table (`results.md`) and every daemon's log are kept in a temporary directory
named on stderr, or in `--out DIR`. `--only lucent,mako`, `--runs N` and
`--idle-seconds S` narrow or shorten a run; `--lucent PATH` picks the binary.

The tests for its parsing and formatting run with
`python3 -m unittest bench/test_benchmark.py`.

## Requirements

Python 3.10+ (standard library only), `dbus-daemon`, `notify-send` (libnotify)
and `gdbus` (glib2). The competitors are found on `PATH`; missing ones are
skipped with a note on stderr. Lucent is taken from `--lucent`, else `PATH`,
else `target/release/lucent`.

## How each daemon is run

Each measurement starts a **private `dbus-daemon`** whose only service file
makes the daemon under test D-Bus activatable as
`org.freedesktop.Notifications`, exactly as a packaged daemon is started on a
real desktop. The desktop's own session bus, and the notification daemon on
it, are never stopped, replaced or contacted, so there is nothing to restore
afterwards and every daemon is measured alone. When a measurement ends the
script kills the bus and every daemon process it activated.

Every daemon gets the same environment: an empty `XDG_CONFIG_HOME` (so each
runs its packaged defaults; dunst and swaync then read their `/etc/xdg`
files), a private `XDG_RUNTIME_DIR`, and the real compositor through a
symlink to its socket in that private directory, so `WAYLAND_DISPLAY` stays a
bare name such as `wayland-1`, as on a real desktop (fnott, for one, builds
its control socket path from it). Lucent runs twice: `lucent` with its default Idle
exit (ADR-0002), and `lucent (resident)` with `idle_exit = false`, which is
the fair row to compare against daemons that stay running.

A daemon that cannot be started, or stops answering, does not end the run:
its row reads `failed` with a footnote naming the measurement and the last
line of its `daemon.log`, and the remaining daemons are still measured.

## What is measured

| Column | Method |
|---|---|
| Version | `pacman -Qo` of the binary; for a Lucent built from this tree, the Cargo version and commit. |
| Binary size | Size on disk of the executable, shared libraries not included (Lucent links Cairo and Pango; swaync links GTK). |
| Idle RSS | `VmRSS` at the end of a window of `--idle-seconds` (default 60 s) that starts `--settle` seconds (default 10 s) after the daemon is activated by a `GetServerInformation` call, without ever showing a Popup. `0 (not running)` means the process had exited: Lucent's Idle exit, after its 5 s grace. |
| Idle wakeups | Voluntary plus involuntary context switches, summed over all the process's threads (`/proc/PID/task/*/status`), across that same window. Each is a time the kernel woke the daemon. |
| Peak RSS, 5 Popups | `VmHWM` of a freshly activated daemon 2 s after five `notify-send` calls with a 15 s timeout. |
| PSS, 5 Popups | `Pss` from `/proc/PID/smaps_rollup` at the same moment: RSS with shared pages (libraries, font caches) divided among the processes that share them, a fairer view than RSS of what the daemon itself costs. |
| Cold start | Median over `--runs` (default 10) of the time from just before `notify-send` is spawned, with no daemon process running, to the daemon's first `wl_surface.commit` that carries a newly attached buffer: the Popup's first frame handed to the compositor. One extra run beforehand warms the page cache and is discarded, so this is a cold *process*, not a cold disk. |
| Warm | The same, with the daemon already running and idle: a warm-up Popup with a 0.5 s timeout is sent, the script waits 1.5 s (inside Lucent's 5 s Idle grace), then measures. |

### How the first frame is seen

The daemon runs with `WAYLAND_DEBUG=1`, which both libwayland (mako, dunst,
fnott, swaync through GTK) and wayland-rs (Lucent) honour, printing every
request with a wall-clock timestamp: `[ms.µs]` (µs since the epoch mod 2^32)
from wayland-rs and libwayland before 1.24, `[HH:MM:SS.µµµµµµ] {queue}` (UTC
time of day) from libwayland 1.24 and later. The script notes the wall-clock
time just before `notify-send` and finds
the first `wl_surface@N.commit()` preceded by a non-nil `attach` to that
surface. Both latencies therefore include `notify-send`'s own start-up and the
D-Bus round trip, identically for every daemon, and stop at the commit, not
at the compositor's presentation of it. Writing the debug log costs the daemon
a little time; the memory and wakeup measurements run without it.

Known limits: a daemon that commits a buffer to some surface at start-up
before it shows the Notification (swaync's hidden control centre could) would
report a cold start that is too short; check its `daemon.log` before trusting
such a number. Latency depends on the compositor; the table states it.

## Hardware

The script prints the CPU, RAM, kernel and `XDG_CURRENT_DESKTOP` above the
table, so a third party can tell whether their machine is comparable.
