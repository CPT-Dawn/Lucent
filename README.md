# Lucent

Lucent is a Wayland notification daemon implementing
`org.freedesktop.Notifications` over the session D-Bus.

## How it works

- `src/main.rs`
	- Loads config from `~/.config/lucent/config.toml` (bootstrapped from `default_config.toml`).
	- Performs a pre-flight D-Bus check: if `org.freedesktop.Notifications` is already owned,
		Lucent exits cleanly.
	- Spawns a dedicated Tokio thread for the D-Bus server and runs GTK on the main thread.
- `src/dbus.rs`
	- Exposes the freedesktop notification interface and signals.
	- Converts incoming D-Bus `Notify` calls to `UiCommand::Show`.
- `src/ui/mod.rs` + `src/ui/window.rs`
	- Manages active windows, queueing, stacking, fade animations, click-to-dismiss,
		and close/action signal relay.
- `src/ui/style.rs`
	- Builds GTK CSS from config values.

## Build and run

```bash
cargo build
cargo run
```

If another daemon already owns `org.freedesktop.Notifications`, Lucent now prints:

```text
[lucent] Another notification daemon is already running (owner: ...).
[lucent] Exiting without error. Stop the existing daemon to run this instance.
```

## Testing

### 1) Check who owns the notification bus name

```bash
busctl --user get-name-owner org.freedesktop.Notifications
```

### 2) Send test notifications

Use either command:

```bash
notify-send "Lucent test" "Hello from notify-send"
```

or

```bash
dbus-send --session --type=method_call --print-reply \
	--dest=org.freedesktop.Notifications \
	/org/freedesktop/Notifications \
	org.freedesktop.Notifications.Notify \
	string:"manual-test" uint32:0 string:"" string:"Summary" string:"Body" \
	array:string:"" dict:string:string:"" int32:5000
```

### 3) Functional checks

- Notification appears top-right with configured width/colors.
- Auto-dismiss respects `timeout_seconds` (or per-notification timeout).
- Clicking dismisses and emits close reason 2.
- More than `max_visible_notifications` are queued and shown in order.

## Install and autostart (systemd user)

1. Build release binary:

```bash
cargo build --release
```

2. Install binary:

```bash
install -Dm755 target/release/lucent ~/.local/bin/lucent
```

3. Create user service file at `~/.config/systemd/user/lucent.service`:

```ini
[Unit]
Description=Lucent Notification Daemon
After=graphical-session.target
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart=%h/.local/bin/lucent
Restart=on-failure
RestartSec=1

[Install]
WantedBy=default.target
```

4. Enable and start:

```bash
systemctl --user daemon-reload
systemctl --user enable --now lucent.service
```

5. Verify:

```bash
systemctl --user status lucent.service
journalctl --user -u lucent.service -f
```

If your distro already starts another daemon (`mako`, `dunst`, etc.), disable it first,
or Lucent will intentionally exit when the D-Bus name is occupied.