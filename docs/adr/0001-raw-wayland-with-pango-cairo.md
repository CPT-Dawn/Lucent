# Raw Wayland + Pango/Cairo instead of a GUI toolkit

Resource usage is Lucent's headline promise, so v2 drops GTK4 (tens of MB resident, a GL renderer, a full widget tree) and draws Popups itself: `smithay-client-toolkit` for layer-shell surfaces and `wl_shm` buffers, Cairo for software rasterisation, and Pango for text. Pango/Cairo were kept over pure-Rust text stacks because they give correct shaping, markup, emoji and font fallback, and are usually already mapped in memory on a desktop; `cosmic-text` builds its own font database (memory + cold-start cost) and `fontdue`-class crates can't shape or fall back.

## Consequences

- Each Popup is drawn once per content change, never per frame. Enter/exit animation is delegated to the compositor (e.g. Hyprland `layerrule = animation`); do not add a per-frame animation loop to Lucent.
- Blur is the compositor's job; Lucent only supplies a translucent buffer and a stable layer namespace.
- Theming is plain config values, not CSS.
