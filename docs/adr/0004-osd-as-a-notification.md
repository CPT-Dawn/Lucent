# The OSD is a kind of Notification, and `lucent osd` borrows the system's tools

Volume and brightness OSDs (SwayOSD, wob, avizo) are usually a second daemon next to the notification daemon. Lucent shows them itself: a Notification that carries a synchronous tag (`x-canonical-private-synchronous` or `x-dunst-stack-tag`) and a `value` hint is the OSD, a single compact Popup placed by `[osd]` apart from the Stack. This is the shape volume scripts written for dunst and mako already send, so they work unchanged, and the OSD inherits Idle exit, Themes and Rules for free. A second daemon or a second D-Bus interface would cost both.

`lucent osd volume|mic|brightness` is a thin client that changes the level and sends that Notification. It does not talk to PipeWire or the backlight itself: it runs `wpctl` and `brightnessctl`, the tools that already own those levels. Linking libpipewire or writing to sysfs would add dependencies and permissions to every install for a feature that runs once per keypress.

## Consequences

- At most one OSD is up: a new one, from any sender, replaces it, and the old one closes as expired. The OSD never waits in the Queue or counts towards `max_visible`.
- A tagged Notification without a `value` is an ordinary Popup, which the tag still updates in place.
- `lucent osd` fails with a clear message where `wpctl` or `brightnessctl` is missing; neither is a package dependency.
