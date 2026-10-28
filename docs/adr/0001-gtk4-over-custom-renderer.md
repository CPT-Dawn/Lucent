# GTK4 over a custom renderer

Lucent renders Popups with GTK4 + gtk4-layer-shell rather than a hand-rolled smithay-client-toolkit + cairo/pango renderer, even though a custom renderer would idle at roughly 5–10 MB instead of GTK4's ~40–60 MB. GTK4 gives us correct text shaping and Pango markup, icon-theme lookup, CSS theming (which user `style.css` and built-in Themes depend on), light/dark switching and frame-clock-driven animations for free; reimplementing those would multiply the code size and the bug surface for a cost users rarely feel. Lightness is pursued instead through an event-driven core with no polling, so an idle Lucent costs memory but essentially no CPU.

## Considered Options

- **smithay-client-toolkit + cairo/pango** (the mako approach): lightest, but every visual feature becomes our own rendering code.
- **iced / libcosmic**: modern Rust UI, but heavier than GTK4 for a handful of small surfaces and with weaker layer-shell maturity.
