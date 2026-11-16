# Changelog

What changed in each release of Lucent. Each version links to its GitHub release, which has the full notes.

## Unreleased

### Changed

- The README is a short landing page now; the reference lives in [docs/](docs/README.md).

## [2.1.1] - 2026-11-09

A bug-fix release from a full audit of the daemon. Nothing to change in your config.

### Fixed

- An SVG icon can no longer freeze or exhaust the daemon: files an SVG references are ignored. (#39)
- `&#X41;` no longer breaks a Popup's body. (#40)
- `file://localhost/...` image URIs work, and a `?query` or `#fragment` is no longer read as part of the filename. (#41)
- `lucent osd volume up` no longer lowers a volume boosted past 100%. (#42)
- `lucent osd` says why when an audio device is missing. (#43)
- The Queue moves up when a Popup becomes the OSD. (#44)
- Tag replacement is deterministic. (#45)
- A broken stderr no longer aborts Lucent, and `lucent --default-config | head` exits cleanly. (#46)
- `busctl introspect`, `Peer.Ping` and untyped `gdbus call` work against Lucent. (#47)

## [2.1.0] - 2026-11-04

### Added

- The OSD: volume, microphone and brightness as one compact bar apart from your notifications, configured in `[osd]`, with the layer namespace `lucent-osd`.
- `lucent osd volume|mic|brightness up|down|mute|show`, through `wpctl` and `brightnessctl`.
- Synchronous Tags: `x-canonical-private-synchronous` and `x-dunst-stack-tag` replace the live Notification with the same Tag.

### Upgrading

- Widen a Hyprland or niri layer rule matching `^lucent$` to `^lucent(-osd)?$`.

## [2.0.1] - 2026-10-30

Lucent 2, a ground-up rewrite. The `v2.0.0` tag points at an earlier build that was never released.

### Added

- Idle exit: Lucent exits a few seconds after the last Popup closes, and D-Bus starts it for the next Notification.
- One thread, Pango and Cairo straight onto Wayland, no toolkit and no async runtime.
- The Midnight Bloom, `light` and `opaque` Presets, Theme overrides and Rules.
- Blur requested through `ext-background-effect-v1`, and text drawn at fractional scales.
- Icons (SVG included), images, markup, progress bars, Action buttons, a Stack with a Queue, and in-place replacement.
- Clicking a Popup brings its app to the front (`xdg-activation`).

### Changed

- v1 configs must be rewritten: v1 keys are logged as unknown and ignored.
- The D-Bus service file is now `dev.cptdawn.Lucent.service`; delete a hand-installed `org.freedesktop.Notifications.service`.

## [1.1.0] - 2026-04-01

### Added

- Blur support.

## [1.0.2] - 2026-03-22

### Added

- Automated AUR releases.

## [1.0.1] - 2026-03-02

## [1.0.0] - 2026-03-02

First release.

[2.1.1]: https://github.com/CPT-Dawn/Lucent/releases/tag/v2.1.1
[2.1.0]: https://github.com/CPT-Dawn/Lucent/releases/tag/v2.1.0
[2.0.1]: https://github.com/CPT-Dawn/Lucent/releases/tag/v2.0.1
[1.1.0]: https://github.com/CPT-Dawn/Lucent/releases/tag/v1.1.0
[1.0.2]: https://github.com/CPT-Dawn/Lucent/releases/tag/v1.0.2
[1.0.1]: https://github.com/CPT-Dawn/Lucent/releases/tag/v1.0.1
[1.0.0]: https://github.com/CPT-Dawn/Lucent/releases/tag/v1.0.0
