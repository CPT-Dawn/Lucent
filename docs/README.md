# Documentation

The guides for using and configuring Lucent. The reference for every config option is [`default_config.toml`](../default_config.toml), also printed by `lucent --default-config`.

| Guide | What it covers |
| --- | --- |
| [Installation](installation.md) | The AUR, release tarballs, building from source, switching from another daemon, upgrading from 1.x |
| [Usage](usage.md) | What a Popup shows, the mouse, the Stack and Queue |
| [Configuration](configuration.md) | The config file, Presets, Theme overrides, layout, Rules |
| [OSD](osd.md) | `lucent osd`, media key binds for Hyprland, Sway and niri, `[osd]` |
| [Compositors](compositors.md) | Blur and enter/exit animation on each compositor |
| [Troubleshooting](troubleshooting.md) | The problems people hit, each with its fix, and the FAQ |
| [Benchmark](benchmark.md) | Memory, wakeups and latency next to mako, dunst, swaync and fnott |

For contributors:

| Page | What it covers |
| --- | --- |
| [GLOSSARY.md](../GLOSSARY.md) | The words the code and docs use: Popup, Stack, Queue, Tag, OSD |
| [adr/](adr/) | The big decisions: raw Wayland with Pango/Cairo, Idle exit, one thread, the OSD |
| [agents/](agents/) | How coding agents use the issue tracker and labels |

[Back to the README](../README.md)
