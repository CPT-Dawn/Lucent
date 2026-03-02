# Lucent

**A blazing-fast, D-Bus activated Wayland notification daemon**

[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Maintenance](https://img.shields.io/badge/Maintained-yes-brightgreen.svg)](https://github.com/CPT-Dawn/Lucent)

![Demo](assets/demo.gif)

Lucent is a Rust implementation of `org.freedesktop.Notifications` for Wayland compositors, built around a bus-activated, event-driven model. The process is not kept resident by default: D-Bus spawns it on first notification delivery, and it remains out of memory while idle.

> [!NOTE]
> Lucent targets Wayland and uses `gtk4-layer-shell`. Your compositor must support layer-shell surfaces (wlroots-based compositors, GNOME with compatible stack, etc.).

## Why Lucent?

Legacy notification daemons were largely shaped by X11-era assumptions: long-lived background processes, implicit compositor coupling, and ad-hoc startup orchestration. Lucent takes the opposite approach:

- **Rust 2021** implementation with memory-safe ownership and explicit concurrency boundaries.
- **Pure D-Bus activation** on `org.freedesktop.Notifications`, without requiring user services or compositor autostart entries.
- **Event-driven architecture** with D-Bus-triggered startup, async signal relay, and compositor-timed GTK animations.
- **Wayland-native rendering** via `gtk4-layer-shell`, with overlay windows anchored for compositor-managed placement.

## Core Features

- Full `org.freedesktop.Notifications` server surface (`Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`).
- Signal emission for `NotificationClosed` and `ActionInvoked` with proper reason codes.
- Replacement semantics for `replaces_id` and bounded queueing when `max_visible_notifications` is reached.
- Delta-time frame-clock animations for entry, exit, and restack transitions.
- Pango-markup body rendering and per-notification expiration policy (`-1`, `0`, or millisecond timeout).
- Embedded config bootloader: auto-writes a default config to XDG config on first launch.

## Installation

### Arch Linux (AUR)

```bash
paru -S lucent-git
```

The package installs both:

- `/usr/bin/lucent`
- `/usr/share/dbus-1/services/org.freedesktop.Notifications.service`

After installation, no `systemctl --user` commands and no compositor startup snippets are required.

### Build from Source

```bash
git clone https://github.com/CPT-Dawn/Lucent.git
cd Lucent
cargo build --release
```

Install the binary:

```bash
install -Dm755 target/release/lucent /usr/bin/lucent
```

Install the D-Bus activation file:

```bash
install -Dm644 org.freedesktop.Notifications.service \
  /usr/share/dbus-1/services/org.freedesktop.Notifications.service
```

> [!NOTE]
> Manual installs to `/usr/bin` and `/usr/share/dbus-1/services` require root privileges.

## Configuration

Lucent uses an embedded-asset bootloader pattern:

1. `default_config.toml` is compiled into the binary.
2. On first launch, Lucent writes `~/.config/lucent/config.toml` if missing.
3. The file is parsed into strongly typed Rust structs before UI startup.

Example configuration:

```toml
# Geometry
width = 350
timeout_seconds = 5

# Colors
background_color = "#0D0B14E6"  # deep translucent base
border_color = "#00D1FF"        # cyan accent
text_color = "#FF5A5F"          # red accent

# Shape & typography
corner_radius = 12
font_family = "Geist"

# Stacking
max_visible_notifications = 5
```

## Usage & IPC

Trigger notifications with any client that speaks the freedesktop spec:

```bash
notify-send "Build complete" "All targets passed"
```

Direct D-Bus invocation is also supported:

```bash
dbus-send --session --type=method_call --print-reply \
  --dest=org.freedesktop.Notifications \
  /org/freedesktop/Notifications \
  org.freedesktop.Notifications.Notify \
  string:"manual-test" uint32:0 string:"" string:"Summary" string:"Body" \
  array:string:"" dict:string:string:"" int32:5000
```

Do-Not-Disturb control flags are **not currently implemented** in the Lucent CLI; there are no DND toggle switches at this time.

## Runtime Architecture

- GTK runs on the main thread for window lifecycle and rendering.
- A dedicated Tokio thread hosts the zbus server and session-bus I/O.
- Channels bridge D-Bus ↔ UI command flow (`UiCommand`, `DbusSignal`) with no polling worker threads.

If another notification daemon already owns `org.freedesktop.Notifications`, Lucent exits cleanly rather than force-stealing the name.