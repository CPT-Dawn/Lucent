# Benchmark

Lucent measured next to mako, dunst, swaync and fnott, with [`bench/benchmark.py`](../bench/README.md), which also explains every column.

Measured on 2026-10-30: Intel Core i7-13700HX, 31 GiB RAM, Linux 7.2.11-arch1-1, Hyprland 0.57.0, with `bench/benchmark.py --runs 10 --idle-seconds 60`. Latencies are medians.

| Daemon | Version | Binary size | Idle RSS | Idle wakeups | Peak RSS, 5 Popups | PSS, 5 Popups | Cold start | Warm |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| **lucent** | 2.0.1 | 1.9 MiB[^size] | **0 (not running)** | **0** | 24.4 MiB | 9.4 MiB | 27.9 ms | 7.3 ms |
| lucent (resident) | 2.0.1 | 1.9 MiB[^size] | 9.3 MiB | 0 | 24.6 MiB | 9.4 MiB | 30.5 ms | 6.7 ms |
| mako | 1.11.0-1 | 91 KiB | 8.8 MiB | 0 | 22.9 MiB | 7.7 MiB | 28.1 ms | 5.8 ms |
| dunst | 1.13.2-3 | 207 KiB | 10.9 MiB | 0 | 30.0 MiB | 10.7 MiB | 14.3 ms[^dunst] | 9.3 ms |
| swaync | 0.12.6-1 | 615 KiB | 47.2 MiB | 0 | 309.1 MiB | 154.1 MiB | 226.4 ms | 40.0 ms |
| fnott | 1.8.0-1 | 187 KiB | 14.6 MiB | 0 | 20.7 MiB | 7.0 MiB | 29.3 ms | 6.3 ms |

[^size]: A plain `cargo build --release`. The AUR package links with packed relocations and comes out about 120 KiB smaller.
[^dunst]: Too good to be true. dunst attaches a buffer to its 24×24 cursor surface as it starts, about 31 ms before it draws the Popup, and the benchmark takes that first commit for the Popup's first frame (a limit noted in [bench/README.md](../bench/README.md)). Its real cold start is nearer 45 ms.

## How it is measured

Each daemon runs alone on a private D-Bus with its packaged defaults. *Idle* is a 60 s window with no Popup shown. *PSS* counts shared library pages fairly. *Cold start* is from `notify-send` to the first frame handed to the compositor with no daemon process running; *warm* is the same with the daemon already up.

## What the table says

- **Idle** is where Lucent wins outright: it has exited, so there is nothing to measure. Kept resident (`idle_exit = false`), it sits with mako, at about 9 MiB.
- **While showing Popups**, fnott and mako use about 2 MiB less PSS than Lucent, and less peak RSS too. Lucent's binary is also the largest here. These figures are at scale 1; on a HiDPI monitor every daemon that draws at full resolution needs more pixels for its buffers.
- **Latency** is on par with mako and fnott, warm or cold. Cold start is the price of Idle exit: the first Notification after a quiet spell starts a process, which costs about 20 ms more than a warm one.

## Run it yourself

```sh
cargo build --release
python3 bench/benchmark.py > results.md
```

It needs a live Wayland session and takes about two minutes per daemon. [bench/README.md](../bench/README.md) has the options and requirements.

[Back to the README](../README.md)
