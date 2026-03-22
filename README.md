# Lucent

A D-Bus activated Wayland notification daemon in Rust with zero resident idle footprint.

[![CI](https://img.shields.io/badge/CI-placeholder-inactive)](#)
[![License](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.XX%2B-orange?logo=rust)](#)

![Lucent Demo](assets/demo.gif)

## Architecture & Philosophy

Lucent implements `org.freedesktop.Notifications` as a D-Bus-activated service (`org.freedesktop.Notifications.service`) rather than a permanently running daemon.

- **Headless activation path:** no `systemd --user` unit and no compositor autostart stanza are required.
- **Zero idle residency:** when no notification is sent, Lucent is not running and consumes no resident memory.
- **Event-driven runtime:** process startup is triggered by D-Bus method calls; notification lifecycle is driven by IPC events.
- **Rust memory safety:** notification state, IPC routing, and rendering are implemented in Rust, with Wayland surfaces via `gtk4-layer-shell`.

Compared to legacy X11-era notification daemons (long-lived background services with startup orchestration), Lucent is bus-activated and compositor-native on Wayland.

> [!WARNING]
> Lucent requires a Wayland session and layer-shell support through the GTK4/layer-shell stack.

## Installation

### Arch Linux (AUR)

#### `lucent`

```bash
paru -S lucent
```

All packages install:

- `lucent` to `/usr/bin/lucent`
- D-Bus activation file to `/usr/share/dbus-1/services/org.freedesktop.Notifications.service`

### Building from Source (Non-Arch)

```bash
git clone https://github.com/CPT-Dawn/Lucent.git
cd Lucent
cargo build --release --locked
```

Install binary and D-Bus service file:

```bash
sudo install -Dm755 target/release/lucent /usr/bin/lucent
sudo install -Dm644 org.freedesktop.Notifications.service \
  /usr/share/dbus-1/services/org.freedesktop.Notifications.service
```

> [!WARNING]
> If `org.freedesktop.Notifications.service` is not installed in a D-Bus service directory, D-Bus activation will not occur.

## Configuration

Configuration is loaded from XDG config location:

- Primary path: `~/.config/lucent/config.toml`
- Source of defaults: embedded `default_config.toml` written automatically on first run

Reference configuration:

```toml
# Lucent configuration
# Location: ~/.config/lucent/config.toml

# Width of each notification popup in pixels.
width = 350

# Default auto-dismiss timeout in seconds.
timeout_seconds = 5

# Window background color (hex, optional alpha).
background_color = "#0D0B14E6"

# Border color (hex).
border_color = "#1A1525"

# Text color (hex).
text_color = "#E8E2F0"

# Corner radius in pixels.
corner_radius = 12

# Font family used for title/body text.
font_family = "Geist"

# Max visible notifications before queueing.
max_visible_notifications = 5
```

## Usage & IPC

Standard freedesktop clients work without Lucent-specific integration:

```bash
notify-send "Build completed" "All targets passed"
```

Raw D-Bus IPC example:

```bash
dbus-send --session --type=method_call --print-reply \
  --dest=org.freedesktop.Notifications \
  /org/freedesktop/Notifications \
  org.freedesktop.Notifications.Notify \
  string:"manual-test" uint32:0 string:"" string:"Summary" string:"Body" \
  array:string:"" dict:string:variant:"" int32:5000
```

CLI surface:

- `lucent` currently exposes no custom command-line flags.
- Lifecycle is managed by D-Bus activation, not by daemon control subcommands.

## Contributing

Pull requests are welcome, but review is strict on correctness and maintainability.

Before opening a PR, ensure all of the following are true:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```

PR requirements:

- Code is fully formatted with `cargo fmt`.
- `cargo clippy` passes with zero warnings.
- Description includes a clear architectural explanation of what changed and why.

## License

MIT. See [LICENSE](LICENSE).