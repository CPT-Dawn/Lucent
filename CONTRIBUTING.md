# Contributing

Issues and pull requests are welcome. This page has the setup, the checks CI runs, how to try a build on your desktop, and how the README's images are made.

## Setup

You need Rust 1.92 or newer and the development files for Cairo, Pango, Wayland and D-Bus (Debian and Ubuntu: `pkg-config libcairo2-dev libpango1.0-dev libwayland-dev libdbus-1-dev dbus`).

```sh
git clone https://github.com/CPT-Dawn/Lucent.git && cd Lucent
cargo build --release
```

## Checks

CI ([`ci.yml`](.github/workflows/ci.yml)) runs these on every push and pull request. Run them before you open one:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --all-targets --features test-hooks -- -D warnings
cargo test --locked
cargo test --locked --features test-hooks
```

The `test-hooks` feature adds a D-Bus interface the bus tests use to click Popups without a compositor. Plain `cargo test` proves release builds lack it. Never enable it in a build you install.

## Trying a build on your desktop

Only one daemon can own the notification bus name. When nothing owns it (`busctl --user status org.freedesktop.Notifications` fails), start your build and send it a Notification:

```sh
LUCENT_LOG=debug ./target/release/lucent
notify-send "Hello" "from my build"
./target/release/lucent osd volume show   # the OSD, without changing the volume
```

Once your build exits, D-Bus activation goes back to the installed `/usr/bin/lucent`.

## Design

The vocabulary is in [GLOSSARY.md](GLOSSARY.md) and the big decisions are in [docs/adr/](docs/adr/). Changes that cost idle memory or wakeups need a very good reason. The [benchmark](docs/benchmark.md) is run by hand before a release; it needs a live Wayland session.

## The README's images

| File | How it is made |
| --- | --- |
| `assets/midnight-bloom.png`, `light.png`, `opaque.png` | The five `notify-send` commands in [Usage](docs/usage.md#try-it), on Hyprland with Lucent's own blur and Breeze icons (Breeze Dark on the dark Presets), captured over a test pattern |
| `assets/osd.png` | `lucent osd volume show` at 65% |
| `assets/logo.svg` | Hand-written SVG in the Midnight Bloom palette |
| `assets/social-preview.png` | The three Preset screenshots side by side, composed with the readme skill's `readme-assets social`; uploaded under Settings → Social preview |

## Pull requests

Use [conventional commits](https://www.conventionalcommits.org/) (`fix(osd): …`, `feat: …`). Add a line to the Unreleased section of [CHANGELOG.md](CHANGELOG.md) for anything a user would notice.
