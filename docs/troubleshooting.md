# Troubleshooting

The problems people hit, each with its fix, and the questions people ask. Lucent logs to stderr; when D-Bus starts it, that is your user journal.

## Nothing shows up

Run Lucent in a terminal with debug logging and send a Notification from another:

```sh
LUCENT_LOG=debug lucent
```

If it says `org.freedesktop.Notifications is already owned by mako (pid 1234)`, another daemon has the bus name; see [Switching](installation.md#switching-from-mako-dunst-or-swaync). When D-Bus starts Lucent, read its log with:

```sh
journalctl --user -b | grep -i lucent
```

## No blur

Blur is your compositor's job. Lucent asks for it where the compositor offers `ext-background-effect-v1`; otherwise add the rule for your compositor from [Compositors](compositors.md). On Hyprland, `hyprctl layers` should list a `lucent` layer while a Popup is showing. With no blur available, `preset = "opaque"` looks intentional rather than see-through.

## Popups are on the wrong monitor

By default the compositor picks, which is normally the focused monitor. Pin one with `[layout] monitor = "<name>"`, using a name from `hyprctl monitors`, `swaymsg -t get_outputs` or `niri msg outputs`. A name that matches nothing is logged.

## My config change did nothing

Lucent reads its config when it starts. If a Popup is still showing, it is still the old process: wait for it to exit (about 5 s after the last Popup), then send another. Check the log for a parse error.

## The first Notification is a little slower than the rest

That is Idle exit starting a fresh process; see the cold-start column in the [benchmark](benchmark.md). If you'd rather pay memory than latency, set `idle_exit = false`.

## `lucent osd` does nothing

Volume needs `wpctl` (WirePlumber) and brightness needs `brightnessctl`. Run the command in a terminal: it says which tool or device is missing.

## Is there a history, do-not-disturb or a control centre?

No. A process that isn't running can't remember anything, and that is the point. If you need those, swaync is excellent.

## GNOME or X11?

No. Lucent needs the layer-shell protocol, which wlroots compositors, Hyprland, niri and KDE Plasma provide.

[Back to the README](../README.md)
