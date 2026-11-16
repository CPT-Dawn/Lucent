# Compositors

Blur and enter/exit animation on each compositor. Lucent draws a translucent Popup and leaves blur and animation to the compositor, so it never draws a frame it doesn't need. Every Popup is a layer surface with the namespace `lucent`, and the OSD's is `lucent-osd`.

Blur needs no setup on compositors that offer the `ext-background-effect-v1` protocol (Hyprland 0.57 does): Lucent asks for it behind every translucent Popup, shaped to its rounded corners. Elsewhere, add the rule for your compositor below. Opaque Themes never ask for blur.

| Compositor | Blur | Enter/exit animation |
| --- | --- | --- |
| Hyprland | yes | yes (`slide`, `popin`, `fade`) |
| SwayFX | yes | no: its `animation_duration_ms` covers windows only |
| Sway | no | no |
| niri | yes (26.04+) | no: niri's animations cover windows, workspaces and its own UI, and no `layer-rule` property animates a layer surface |

Lucent won't fake an animation on the compositors that lack one: drawing its own frames would cost the wakeups it exists to avoid. On those, Popups appear and disappear at once.

Lucent needs the layer-shell protocol, which wlroots compositors, Hyprland, niri and KDE Plasma provide. GNOME and X11 are not supported.

## Hyprland

Hyprland 0.55 and later (`hyprland.lua`):

```lua
hl.layer_rule({
    name         = "lucent",
    match        = { namespace = "^lucent(-osd)?$" },
    blur         = true,
    ignore_alpha = 0.1,
    animation    = "slide right",
})
```

Hyprland 0.53 and 0.54 (`hyprland.conf`):

```ini
layerrule {
    name = lucent
    match:namespace = ^lucent(-osd)?$
    blur = on
    ignore_alpha = 0.1
    animation = slide right
}
```

Blur itself must be on (`decoration.blur.enabled`, the default). Use `slide left` for a left anchor, `slide top` or `slide bottom` for the centre ones, or `popin 80%` or `fade`.

## Sway

Plain Sway has no blur. [SwayFX](https://github.com/WillPower3309/swayfx) does:

```
blur enable
layer_effects "lucent" {
    blur enable;
    blur_ignore_transparent enable;
    corner_radius 20;
}
```

Neither animates layer surfaces: SwayFX's `animation_duration_ms` applies to opening and closing windows only, and `layer_effects` has no animation setting.

## niri

niri 26.04 and later:

```kdl
layer-rule {
    match namespace="^lucent(-osd)?$"
    geometry-corner-radius 20
    background-effect {
        blur true
    }
}
```

`geometry-corner-radius` rounds the blur to match the Popup; change it if you change `radius`. niri has no layer-surface animation: none of its `animations` (`window-open`, `window-close` and the rest) applies to layer surfaces, and `layer-rule` has no animation property.

[Back to the README](../README.md)
