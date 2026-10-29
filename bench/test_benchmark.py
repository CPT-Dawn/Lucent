"""Tests for the pure parts of benchmark.py: `python3 -m unittest bench/test_benchmark.py`."""

import os
import shutil
import socket
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import benchmark as bench  # noqa: E402

# A Lucent (wayland-rs) log: startup chatter, then the first Popup frame.
RS_LOG = """\
[1000000.000][rs] -> wl_display@1.get_registry(new id wl_registry@2)
[1000010.500][rs] -> wl_compositor@4.create_surface(new id wl_surface@9)
[1000011.000][rs] -> wl_surface@9.commit()
[1000020.000][rs] <- zwlr_layer_surface_v1@10.configure(1, 360, 80)
[1000021.000][rs] -> wl_surface@9.attach(Some(wl_buffer@12), 0, 0)
[1000021.250][rs] -> wl_surface@9.damage_buffer(0, 0, 360, 80)
[1000022.750][rs] -> wl_surface@9.commit()
[1000030.000][rs] -> wl_surface@9.attach(Some(wl_buffer@13), 0, 0)
[1000031.000][rs] -> wl_surface@9.commit()
"""

# The same shape as libwayland prints it (mako, dunst, fnott, swaync).
C_LOG = """\
[2000000.000]  -> wl_compositor#4.create_surface(new id wl_surface#20)
[2000001.000]  -> wl_surface#20.attach(nil, 0, 0)
[2000002.000]  -> wl_surface#20.commit()
[2000005.000]  -> wl_surface#20.attach(wl_buffer#31, 0, 0)
[2000005.500]  -> wl_surface#20.commit()
"""


class FirstFrame(unittest.TestCase):
    def test_wayland_rs_log_gives_the_first_commit_carrying_a_buffer(self):
        # t0 at 999999.000 ms; first buffer commit at 1000022.750 ms.
        self.assertAlmostEqual(bench.first_frame_ms(RS_LOG, 999_999_000), 23.75)

    def test_libwayland_log_skips_the_nil_attach(self):
        self.assertAlmostEqual(bench.first_frame_ms(C_LOG, 2_000_000_000), 5.5)

    def test_commits_before_t0_do_not_count(self):
        self.assertAlmostEqual(bench.first_frame_ms(RS_LOG, 1_000_025_000), 6.0)

    def test_survives_the_32_bit_microsecond_wrap(self):
        # Stamps are (µs since epoch) mod 2^32; t0 falls just before the wrap.
        log = (
            "[      1.000]  -> wl_surface#3.attach(wl_buffer#4, 0, 0)\n"
            "[      2.000]  -> wl_surface#3.commit()\n"
        )
        self.assertAlmostEqual(bench.first_frame_ms(log, 2**32 - 3_000), 5.0)

    def test_libwayland_1_24_time_of_day_stamps_with_queue_names(self):
        # Newer libwayland prints UTC time of day and the event queue's name.
        # t0 is 2026-10-29 16:13:47.250000 UTC, in µs since the epoch.
        t0 = 1_793_290_427_250_000
        log = (
            "[16:13:47.240000] {Default Queue}  -> wl_surface#16.attach(wl_buffer#9, 0, 0)\n"
            "[16:13:47.245000] {Default Queue}  -> wl_surface#16.commit()\n"
            "[16:13:47.291034] {Default Queue}  -> wl_surface#16.attach(wl_buffer#15, 0, 0)\n"
            "[16:13:47.291036]  -> wl_surface#16.commit()\n"
        )
        self.assertAlmostEqual(bench.first_frame_ms(log, t0), 41.036)

    def test_time_of_day_stamps_survive_midnight(self):
        t0 = 1_793_318_399_990_000  # 23:59:59.990000 UTC
        log = (
            "[00:00:00.001000]  -> wl_surface#3.attach(wl_buffer#4, 0, 0)\n"
            "[00:00:00.002000]  -> wl_surface#3.commit()\n"
        )
        self.assertAlmostEqual(bench.first_frame_ms(log, t0), 12.0)

    def test_no_frame_is_none(self):
        self.assertIsNone(bench.first_frame_ms("[1.000]  -> wl_surface#3.commit()\n", 0))


MAIN_STATUS = """\
Name:\tlucent
VmHWM:\t   14336 kB
VmRSS:\t   12288 kB
Threads:\t2
voluntary_ctxt_switches:\t40
nonvoluntary_ctxt_switches:\t2
"""
HELPER_STATUS = """\
Name:\tpango-fc
voluntary_ctxt_switches:\t7
nonvoluntary_ctxt_switches:\t1
"""


class Wakeups(unittest.TestCase):
    def test_counts_every_context_switch_of_every_thread(self):
        self.assertEqual(bench.context_switches([MAIN_STATUS, HELPER_STATUS]), 50)

    def test_no_threads_means_no_wakeups(self):
        self.assertEqual(bench.context_switches([]), 0)


class StatusFields(unittest.TestCase):
    def test_reads_kib_fields_as_bytes(self):
        self.assertEqual(bench.status_kib(MAIN_STATUS, "VmHWM"), 14336 * 1024)

    def test_reads_smaps_rollup_too(self):
        rollup = "55d0-7ffd ---p 00000000 00:00 0 [rollup]\nRss: 9000 kB\nPss:  3456 kB\n"
        self.assertEqual(bench.status_kib(rollup, "Pss"), 3456 * 1024)

    def test_a_missing_field_is_none(self):
        self.assertIsNone(bench.status_kib(HELPER_STATUS, "VmRSS"))


class Table(unittest.TestCase):
    def test_renders_a_readme_ready_markdown_table(self):
        rows = [
            bench.Result(
                daemon="lucent",
                version="1.1.0",
                binary_bytes=1_572_864,
                idle_rss=None,
                idle_wakeups=0,
                peak_rss=19_189_760,
                popups_pss=7_340_032,
                cold_ms=[40.0, 44.5, 41.0],
                warm_ms=[3.0, 2.5],
            ),
            bench.Result(
                daemon="mako",
                version="1.9.0-1",
                binary_bytes=180_224,
                idle_rss=5_242_880,
                idle_wakeups=3,
                peak_rss=9_437_184,
                popups_pss=None,
                cold_ms=[],
                warm_ms=[1.25],
            ),
        ]
        self.assertEqual(
            bench.markdown_table(rows),
            "| Daemon | Version | Binary size | Idle RSS | Idle wakeups "
            "| Peak RSS, 5 Popups | PSS, 5 Popups | Cold start | Warm |\n"
            "|---|---|--:|--:|--:|--:|--:|--:|--:|\n"
            "| lucent | 1.1.0 | 1.5 MiB | 0 (not running) | 0 | 18.3 MiB | 7.0 MiB "
            "| 41.0 ms | 2.8 ms |\n"
            "| mako | 1.9.0-1 | 176 KiB | 5.0 MiB | 3 | 9.0 MiB | n/a | n/a "
            "| 1.2 ms |\n",
        )


class FailedRow(unittest.TestCase):
    def test_a_daemon_that_failed_gets_a_row_and_a_footnote(self):
        row = bench.Result(
            daemon="fnott",
            version="1.7.1-1",
            binary_bytes=204_800,
            idle_rss=None,
            idle_wakeups=0,
            peak_rss=None,
            popups_pss=None,
            error="cold start: notify-send exited 1",
        )
        self.assertEqual(
            bench.markdown_table([row]).splitlines()[2:],
            [
                "| fnott | 1.7.1-1 | 200 KiB | failed¹ | failed¹ | failed¹ "
                "| failed¹ | failed¹ | failed¹ |",
                "",
                "¹ fnott: cold start: notify-send exited 1",
            ],
        )


@unittest.skipUnless(
    os.environ.get("XDG_RUNTIME_DIR")
    and all(shutil.which(t) for t in ("dbus-daemon", "notify-send", "gdbus")),
    "needs a session runtime dir, dbus-daemon, notify-send and gdbus",
)
class DaemonThatCannotStart(unittest.TestCase):
    """Runs a real private bus; the "daemon" exits at once, as fnott did."""

    def test_is_reported_as_a_failed_row_instead_of_ending_the_run(self):
        script = Path(self.enterContext(tempfile.TemporaryDirectory())) / "broken"
        script.write_text("#!/bin/sh\necho 'err: cannot bind' >&2\nexit 1\n")
        script.chmod(0o755)
        root = Path(self.enterContext(tempfile.TemporaryDirectory()))

        result = bench.measure_daemon(
            bench.Daemon("broken", str(script)), root, runs=1, settle=0, idle_seconds=0
        )

        self.assertEqual(result.daemon, "broken")
        self.assertIn("cold start", result.error)
        self.assertIn("err: cannot bind", result.error)


class CompositorLink(unittest.TestCase):
    """fnott names its control socket `$XDG_RUNTIME_DIR/fnott-$WAYLAND_DISPLAY.sock`,
    so WAYLAND_DISPLAY must be a bare name that resolves in the private runtime."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.real = Path(tmp.name) / "real"
        self.private = Path(tmp.name) / "private"
        self.real.mkdir()
        self.private.mkdir()
        self.server = socket.socket(socket.AF_UNIX)
        self.addCleanup(self.server.close)
        self.server.bind(str(self.real / "wayland-1"))
        self.server.listen()

    def assert_reaches_compositor(self, env):
        self.assertNotIn("/", env["WAYLAND_DISPLAY"])
        client = socket.socket(socket.AF_UNIX)
        self.addCleanup(client.close)
        client.connect(str(self.private / env["WAYLAND_DISPLAY"]))

    def test_a_relative_display_is_linked_into_the_private_runtime(self):
        env = bench.wayland_env(str(self.real), "wayland-1", self.private)
        self.assertEqual(env, {"WAYLAND_DISPLAY": "wayland-1"})
        self.assert_reaches_compositor(env)

    def test_an_absolute_display_is_linked_by_its_basename(self):
        env = bench.wayland_env("/nowhere", str(self.real / "wayland-1"), self.private)
        self.assert_reaches_compositor(env)


if __name__ == "__main__":
    unittest.main()
