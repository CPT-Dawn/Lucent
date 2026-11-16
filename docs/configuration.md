# Configuration

How to change Lucent's look and behaviour. The file is `~/.config/lucent/config.toml` (`$XDG_CONFIG_HOME/lucent/config.toml`), and the reference for every option, with its default, is [`default_config.toml`](../default_config.toml).

Lucent needs no config file. Create one and set only what you want; every key you leave out keeps its default. Lucent reads the file each time it starts, so changes apply from the next Notification.

```sh
lucent --default-config   # every option, commented, with its default
```

The same file is installed at `/usr/share/doc/lucent/config.toml`. A typo never silences your desktop: a broken file is logged and the defaults are used, and unknown keys are logged and skipped.

## Presets

```toml
[theme]
preset = "light"   # "midnight-bloom" (default), "light" or "opaque"
```

| Preset | Look | Use it when |
| --- | --- | --- |
| `midnight-bloom` | Dark smoked glass, mint accents | Your compositor blurs (it's still readable if not) |
| `light` | Paper-white glass, ink text | You have a light desktop |
| `opaque` | Midnight Bloom on solid ground | Your compositor has no blur |

## Theme overrides

Any Theme value can be set on top of the Preset:

```toml
[theme]
preset = "midnight-bloom"
font = "JetBrains Mono, monospace"
background = "rgba(20, 27, 31, 0.70)"
accent = "#FF9E64"
radius = 12
```

Colours are `"#RRGGBB"`, `"#RRGGBBAA"`, `"rgb(r, g, b)"` or `"rgba(r, g, b, a)"`. The default font is `Inter Variable, Inter, sans-serif` (Arch: `pacman -S inter-font`); the variable font keeps memory down, and without Inter installed your `sans-serif` is used.

## Layout and behaviour

```toml
[general]
timeout = 8000             # ms; apps may ask for their own (honor_app_timeouts)
max_visible = 3
right_click = "dismiss"    # instead of "dismiss-all"

[layout]
anchor = "bottom-center"
width = 400
monitor = "DP-1"           # unset = the compositor's choice, normally the focused monitor

[layout.margin]
bottom = 48
```

The defaults are a 5000 ms timeout, five Popups, a `top-right` anchor and a width of 360.

## Rules

Rules restyle, or retime, the Notifications that match an app name and/or an Urgency. Later Rules win.

```toml
[[rule]]
app_name = "Spotify"
timeout = 3000
theme = { summary_size = 13, accent = "#1DB954" }

[[rule]]
urgency = "critical"
theme = { critical = "#FF9E64", border_width = 2 }
```

## The OSD

The `[osd]` section places, times or turns off the volume and brightness OSD. See [OSD](osd.md#configure-it).

## Staying resident

Idle exit trades a cold start on the first Notification after a quiet spell for zero idle cost. To keep Lucent running instead:

```toml
[general]
idle_exit = false
```

[Back to the README](../README.md)
