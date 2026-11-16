# Installation

Every way to install Lucent, how to switch from another notification daemon, and how to upgrade from Lucent 1.x. Whichever route you take, the result is the binary at `/usr/bin/lucent` and the D-Bus activation file [`dev.cptdawn.Lucent.service`](../dev.cptdawn.Lucent.service).

## Arch Linux

```sh
paru -S lucent
```

The AUR package builds from source with Cargo, which takes a minute or two. It also installs the reference config at `/usr/share/doc/lucent/config.toml`.

## Release tarball

Each [release](https://github.com/CPT-Dawn/Lucent/releases/latest) has `lucent-<version>-x86_64.tar.gz` and `lucent-<version>-aarch64.tar.gz`, built on Ubuntu 24.04, so they need glibc 2.39 or newer plus Cairo, Pango and D-Bus.

```sh
sha256sum -c lucent-*.tar.gz.sha256
tar xzf lucent-*.tar.gz && cd lucent-*/
sudo install -Dm755 lucent /usr/bin/lucent
sudo install -Dm644 dev.cptdawn.Lucent.service \
  /usr/share/dbus-1/services/dev.cptdawn.Lucent.service
```

## From source

You need Rust 1.92 or newer and the development files for Cairo, Pango and D-Bus (Debian and Ubuntu: `libcairo2-dev libpango1.0-dev libdbus-1-dev pkg-config`).

```sh
git clone https://github.com/CPT-Dawn/Lucent.git
cd Lucent
cargo build --release --locked
sudo install -Dm755 target/release/lucent /usr/bin/lucent
sudo install -Dm644 dev.cptdawn.Lucent.service \
  /usr/share/dbus-1/services/dev.cptdawn.Lucent.service
```

The service file starts `/usr/bin/lucent`; edit its `Exec=` line if you install elsewhere. Without it D-Bus can't start Lucent.

## Optional tools for the OSD

`lucent osd` changes volume through `wpctl` and brightness through `brightnessctl`. Neither is a package dependency, so install the ones you need:

```sh
paru -S wireplumber brightnessctl
```

Most PipeWire desktops already have `wireplumber`.

## No autostart

There is nothing to add to your compositor config: no `exec-once`, no systemd unit, no `spawn-at-startup`. The D-Bus activation file means the first Notification starts Lucent, and Lucent exits again when it has nothing left to show.

If another notification daemon is running, Lucent steps aside and logs which one holds the bus name.

## Switching from mako, dunst or swaync

Only one daemon can own the notification bus name, and when several are installed, which one D-Bus starts is not something you choose. So remove the old one:

1. Remove whatever starts it: an `exec-once` line (Hyprland), an `exec` line (Sway) or a `spawn-at-startup` line (niri), and its systemd unit if you enabled one:

   ```sh
   systemctl --user disable --now mako.service   # or dunst / swaync / fnott
   ```

2. Uninstall it, which also removes its D-Bus activation file:

   ```sh
   paru -Rns mako   # or dunst / swaync / fnott
   ```

3. Stop it if it is still running (`pkill mako`), or log out and back in.

Then `notify-send hello` should show a Lucent Popup. Your old config is not read; Lucent's options are in `lucent --default-config`.

## Upgrading from Lucent 1.x

v2 is a clean break. v1 config keys such as `background_color` or `timeout_seconds` are logged as unknown and ignored, so start again from `lucent --default-config`.

v1 installed its D-Bus file as `org.freedesktop.Notifications.service`. The AUR package replaces it with `dev.cptdawn.Lucent.service`, but if you installed v1 by hand, delete the old one (after checking it says `Exec=/usr/bin/lucent`):

```sh
sudo rm /usr/share/dbus-1/services/org.freedesktop.Notifications.service
```

[Back to the README](../README.md)
