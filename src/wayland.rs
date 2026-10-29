//! The Wayland renderer: keeps one layer-shell surface per Popup in step with
//! the Stack the core asks for, and fills each from a `wl_shm` buffer.
//!
//! A Popup is drawn once per configure, never per frame; animation is the
//! compositor's job (ADR-0001). Where the Stack sits comes from `[layout]`:
//! the first Popup touches the anchor and each next one sits further away.
//!
//! Pointer input on a Popup becomes core [`Event`]s, collected during
//! dispatch for the app to drain with [`Wayland::take_events`]. Popups never
//! take keyboard focus.
//!
//! On a scaled monitor each buffer is drawn at the monitor's scale, so text
//! stays sharp: fractional scales through `wp_fractional_scale_v1` and a
//! `wp_viewport`, whole ones through `wl_surface.set_buffer_scale` where
//! those are missing. Sizes everywhere else are in surface (logical) pixels.

use std::error::Error;

use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::dispatch2::Dispatch2;
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{
    Shape, WpCursorShapeDeviceV1,
};
use smithay_client_toolkit::reexports::protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::config::{self, Layout};
use crate::core::{Button, Event, Popup};
use crate::icon::Icons;
use crate::{debug, draw, error};

/// The layer namespace compositor rules can target.
const NAMESPACE: &str = "lucent";

/// Scales are kept in 120ths, as `wp_fractional_scale_v1` sends them.
const UNSCALED: u32 = 120;

pub struct Wayland {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    pointer: Option<wl_pointer::WlPointer>,
    /// Sets the arrow over Popups, where the compositor offers it; without
    /// it the pointer keeps whatever image it had.
    cursor_shapes: Option<CursorShapeManager>,
    cursor: Option<WpCursorShapeDeviceV1>,
    /// Both present, or neither: fractional scaling needs the two together.
    fractional: Option<(WpFractionalScaleManagerV1, WpViewporter)>,
    /// Pointer input waiting for the app, oldest first.
    events: Vec<Event>,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    qh: QueueHandle<Wayland>,
    layout: Layout,
    /// Set once a missing `[layout] monitor` has been reported, so the log
    /// says it once rather than per Popup.
    reported_monitor: bool,
    /// In Stack order: the first is closest to the anchor.
    surfaces: Vec<Surface>,
}

struct Surface {
    popup: Popup,
    /// The Popup's image, loaded once at the icon size and scale; dropped
    /// with it.
    icon: Option<cairo::ImageSurface>,
    layer: LayerSurface,
    /// The scale buffers are drawn at, in 120ths: 120 until the compositor
    /// says otherwise.
    scale: u32,
    /// Present with fractional scaling: maps the scaled buffer back onto
    /// the surface size.
    viewport: Option<WpViewport>,
    fractional: Option<WpFractionalScaleV1>,
    width: u32,
    height: u32,
    /// Distance from the anchor edge, including the outer margin.
    offset: i32,
    /// Whether the first configure has arrived, so drawing is allowed.
    configured: bool,
    /// Where the Actions sit, kept so pointer motion needs no layout.
    action_rects: Vec<draw::Rect>,
    /// The Action under the pointer, drawn in the accent colour.
    hovered: Option<usize>,
}

impl Drop for Surface {
    fn drop(&mut self) {
        // wayland-rs never destroys a protocol object on its own; these
        // must go before the layer surface (dropped next) destroys theirs.
        if let Some(viewport) = self.viewport.take() {
            viewport.destroy();
        }
        if let Some(fractional) = self.fractional.take() {
            fractional.destroy();
        }
    }
}

/// `logical` surface pixels at `scale` (in 120ths), rounded half away from
/// zero as `wp_fractional_scale_v1` asks.
fn scaled(logical: u32, scale: u32) -> u32 {
    let pixels = (u64::from(logical) * u64::from(scale) + u64::from(UNSCALED / 2)) / 120;
    u32::try_from(pixels).unwrap_or(u32::MAX)
}

/// User data for the scaling objects Lucent binds itself.
struct Scaling;

impl Wayland {
    /// Connects to the compositor named by `WAYLAND_DISPLAY`. The returned
    /// source must be inserted into the event loop, dispatching into this
    /// `Wayland`.
    pub fn connect(layout: Layout) -> Result<(Self, WaylandSource<Wayland>), Box<dyn Error>> {
        let conn = Connection::connect_to_env()?;
        let (globals, mut queue) = registry_queue_init::<Wayland>(&conn)?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|e| format!("the compositor has no layer-shell support: {e}"))?;
        let shm = Shm::bind(&globals, &qh)?;
        let pool = SlotPool::new((layout.width.max(1) * 120 * 4) as usize, &shm)?;
        let fractional = globals
            .bind(&qh, 1..=1, Scaling)
            .and_then(|manager| Ok((manager, globals.bind(&qh, 1..=1, Scaling)?)))
            .ok();
        let mut wayland = Self {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            seats: SeatState::new(&globals, &qh),
            pointer: None,
            cursor_shapes: CursorShapeManager::bind(&globals, &qh).ok(),
            cursor: None,
            fractional,
            events: Vec::new(),
            compositor,
            layer_shell,
            shm,
            pool,
            qh,
            layout,
            reported_monitor: false,
            surfaces: Vec::new(),
        };
        // One more roundtrip so output names are known before the first Popup.
        queue.roundtrip(&mut wayland)?;
        Ok((wayland, WaylandSource::new(conn, queue)))
    }

    /// Makes the on-screen Popups match `stack`, nearest the anchor first.
    /// Popups whose content changed are redrawn in place; the rest are only
    /// moved, and only if their position changed.
    pub fn show(&mut self, stack: &[Popup]) {
        // Dropping a LayerSurface destroys it.
        self.surfaces
            .retain(|s| stack.iter().any(|p| p.id == s.popup.id));
        let mut offset = self.edge_margin();
        for (index, popup) in stack.iter().enumerate() {
            match self.surfaces.iter().position(|s| s.popup.id == popup.id) {
                Some(at) => {
                    self.surfaces.swap(index, at);
                    self.update(index, popup, offset);
                }
                None => {
                    let surface = self.open(popup.clone(), offset);
                    self.surfaces.insert(index, surface);
                }
            }
            offset += px(self.surfaces[index].height + self.layout.gap);
        }
    }

    /// Pointer input on Popups since the last call, as core events.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// The margin on the edge the Stack grows from.
    fn edge_margin(&self) -> i32 {
        let margin = self.layout.margin;
        px(if self.layout.anchor.is_top() {
            margin.top
        } else {
            margin.bottom
        })
    }

    /// The output named by `[layout] monitor`, or `None` for the
    /// compositor's choice (normally the focused monitor).
    fn output(&mut self) -> Option<wl_output::WlOutput> {
        let name = self.layout.monitor.as_deref()?;
        let found = self.outputs.outputs().find(|o| {
            self.outputs
                .info(o)
                .is_some_and(|info| info.name.as_deref() == Some(name))
        });
        if found.is_some() {
            self.reported_monitor = false;
        } else if !self.reported_monitor {
            let known: Vec<String> = self
                .outputs
                .outputs()
                .filter_map(|o| self.outputs.info(&o).and_then(|i| i.name))
                .collect();
            error!(
                "no monitor named `{name}` (have: {}); letting the compositor choose",
                known.join(", ")
            );
            self.reported_monitor = true;
        }
        found
    }

    /// Brings an open Popup up to date with `popup` at `offset`.
    fn update(&mut self, index: usize, popup: &Popup, offset: i32) {
        let layout = &self.layout;
        let surface = &mut self.surfaces[index];
        let mut commit = false;
        let mut redraw = false;
        if surface.popup != *popup {
            let theme = (&popup.theme.icon_size, &popup.theme.icon_theme);
            if surface.popup.image != popup.image
                || theme
                    != (
                        &surface.popup.theme.icon_size,
                        &surface.popup.theme.icon_theme,
                    )
            {
                surface.icon = load_icon(popup, surface.scale);
            }
            surface.popup = popup.clone();
            // The Actions may have moved; the next motion finds the right one.
            surface.hovered = None;
            surface.action_rects = draw::action_rects(popup, surface.icon.as_ref(), surface.width);
            let height = draw::measure(popup, surface.icon.as_ref(), surface.width);
            if height != surface.height {
                // The configure answering this commit redraws at the new size.
                surface.height = height;
                surface.layer.set_size(surface.width, height);
                commit = true;
            } else {
                redraw = surface.configured;
            }
        }
        if surface.offset != offset {
            surface.offset = offset;
            set_margin(&surface.layer, layout, offset);
            commit = true;
        }
        if redraw {
            // Drawing commits, which also carries any new margin.
            if let Err(e) = self.draw(index) {
                error!("could not redraw popup: {e}");
            }
        } else if commit {
            surface.layer.commit();
        }
    }

    fn open(&mut self, popup: Popup, offset: i32) -> Surface {
        let width = self.layout.width;
        let icon = load_icon(&popup, UNSCALED);
        let height = draw::measure(&popup, icon.as_ref(), width);
        let output = self.output();
        let surface = self.compositor.create_surface(&self.qh);
        let (viewport, fractional) = match &self.fractional {
            Some((manager, viewporter)) => (
                Some(viewporter.get_viewport(&surface, &self.qh, Scaling)),
                Some(manager.get_fractional_scale(&surface, &self.qh, Scaling)),
            ),
            None => (None, None),
        };
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            Layer::Overlay,
            Some(NAMESPACE),
            output.as_ref(),
        );
        layer.set_anchor(anchor(self.layout.anchor));
        set_margin(&layer, &self.layout, offset);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(width, height);
        // The first commit carries no buffer; drawing waits for configure.
        layer.commit();
        debug!("opened popup {} ({width}x{height})", popup.id);
        Surface {
            action_rects: draw::action_rects(&popup, icon.as_ref(), width),
            popup,
            icon,
            layer,
            scale: UNSCALED,
            viewport,
            fractional,
            width,
            height,
            offset,
            configured: false,
            hovered: None,
        }
    }

    fn draw(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        let surface = &self.surfaces[index];
        let (bw, bh) = (
            scaled(surface.width, surface.scale),
            scaled(surface.height, surface.scale),
        );
        let (w, h) = (i32::try_from(bw)?, i32::try_from(bh)?);
        let stride = cairo::Format::ARgb32.stride_for_width(bw)?;
        let (buffer, canvas) = self
            .pool
            .create_buffer(w, h, stride, wl_shm::Format::Argb8888)?;
        // Cairo's ARgb32 is premultiplied native-endian ARGB, which is what
        // wl_shm's Argb8888 means on little-endian: no conversion needed.
        // SAFETY: `canvas` holds `stride * h` bytes and stays borrowed until
        // the Cairo surface is finished below, before the buffer is attached.
        unsafe {
            let target = cairo::ImageSurface::create_for_data_unsafe(
                canvas.as_mut_ptr(),
                cairo::Format::ARgb32,
                w,
                h,
                stride,
            )?;
            let cr = cairo::Context::new(&target)?;
            cr.scale(
                f64::from(bw) / f64::from(surface.width.max(1)),
                f64::from(bh) / f64::from(surface.height.max(1)),
            );
            let painted = draw::paint(
                &cr,
                &surface.popup,
                surface.icon.as_ref(),
                surface.width,
                surface.height,
                surface.hovered,
            );
            drop(cr);
            target.finish();
            painted?;
        }
        let wl_surface = surface.layer.wl_surface();
        match &surface.viewport {
            Some(viewport) => {
                let size = |v: u32| i32::try_from(v).unwrap_or(i32::MAX);
                viewport.set_destination(size(surface.width), size(surface.height));
            }
            // Only whole scales get here; 0 must never be sent.
            None => wl_surface.set_buffer_scale((surface.scale / UNSCALED).max(1) as i32),
        }
        wl_surface.damage_buffer(0, 0, w, h);
        buffer.attach_to(wl_surface)?;
        surface.layer.commit();
        Ok(())
    }
}

/// The Popup's image at its Theme's icon size and `scale` (in 120ths), if
/// it has one that loads.
fn load_icon(popup: &Popup, scale: u32) -> Option<cairo::ImageSurface> {
    let image = popup.image.as_ref()?;
    Icons::new(popup.theme.icon_size, &popup.theme.icon_theme)
        .with_scale(f64::from(scale) / f64::from(UNSCALED))
        .load(image)
}

fn anchor(anchor: config::Anchor) -> Anchor {
    use config::Anchor as A;
    match anchor {
        A::TopLeft => Anchor::TOP | Anchor::LEFT,
        // Touching only one edge centres the surface along it.
        A::TopCenter => Anchor::TOP,
        A::TopRight => Anchor::TOP | Anchor::RIGHT,
        A::BottomLeft => Anchor::BOTTOM | Anchor::LEFT,
        A::BottomCenter => Anchor::BOTTOM,
        A::BottomRight => Anchor::BOTTOM | Anchor::RIGHT,
    }
}

/// Places a Popup `offset` pixels from the edge the Stack grows from; the
/// other edges keep their configured margins.
fn set_margin(layer: &LayerSurface, layout: &Layout, offset: i32) {
    let config::Margin {
        top,
        right,
        bottom,
        left,
    } = layout.margin;
    let (top, bottom) = if layout.anchor.is_top() {
        (offset, px(bottom))
    } else {
        (px(top), offset)
    };
    layer.set_margin(top, px(right), bottom, px(left));
}

/// Layer-shell margins are `i32`; config values sit far below its limit.
fn px(value: u32) -> i32 {
    value.try_into().unwrap_or(i32::MAX)
}

impl LayerShellHandler for Wayland {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        debug!("compositor closed a popup surface");
        self.surfaces.retain(|s| &s.layer != layer);
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        let Some(index) = self.surfaces.iter().position(|s| &s.layer == layer) else {
            return;
        };
        let surface = &mut self.surfaces[index];
        surface.configured = true;
        // Zero means "pick your own size": keep ours.
        if configure.new_size.0 != 0 && configure.new_size.0 != surface.width {
            surface.width = configure.new_size.0;
            surface.action_rects =
                draw::action_rects(&surface.popup, surface.icon.as_ref(), surface.width);
        }
        if configure.new_size.1 != 0 {
            surface.height = configure.new_size.1;
        }
        if let Err(e) = self.draw(index) {
            error!("could not draw popup: {e}");
        }
    }
}

impl Wayland {
    /// Redraws the Popup on `surface` at `scale` (in 120ths), if that is
    /// new: its icon is reloaded to match, while its size, in surface
    /// pixels, stays the same.
    fn rescale(&mut self, surface: &wl_surface::WlSurface, scale: u32) {
        let Some(index) = self
            .surfaces
            .iter()
            .position(|s| s.layer.wl_surface() == surface)
        else {
            return;
        };
        let target = &mut self.surfaces[index];
        if scale == 0 || target.scale == scale {
            return;
        }
        debug!("popup {} now drawn at scale {scale}/120", target.popup.id);
        target.scale = scale;
        target.icon = load_icon(&target.popup, scale);
        if target.configured
            && let Err(e) = self.draw(index)
        {
            error!("could not redraw popup: {e}");
        }
    }
}

impl CompositorHandler for Wayland {
    /// The whole-number scale, used only where fractional scaling is missing.
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if self.fractional.is_none() {
            let factor = u32::try_from(factor).unwrap_or(1);
            self.rescale(surface, factor.saturating_mul(UNSCALED));
        }
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    // Frame callbacks are never requested: Popups are drawn once, not per frame.
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Wayland {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl SeatHandler for Wayland {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seats.get_pointer(qh, &seat) {
                Ok(pointer) => {
                    self.cursor = self
                        .cursor_shapes
                        .as_ref()
                        .map(|shapes| shapes.get_shape_device(&pointer, qh));
                    self.pointer = Some(pointer);
                }
                Err(e) => error!("could not use the pointer: {e}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            if let Some(cursor) = self.cursor.take() {
                cursor.destroy();
            }
            pointer.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Wayland {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let Some(index) = self
                .surfaces
                .iter()
                .position(|s| s.layer.wl_surface() == &event.surface)
            else {
                continue;
            };
            let (x, y) = event.position;
            let id = self.surfaces[index].popup.id;
            let event = match event.kind {
                PointerEventKind::Enter { serial } => {
                    if let Some(cursor) = &self.cursor {
                        cursor.set_shape(serial, Shape::Default);
                    }
                    self.hover_action(index, x, y);
                    Event::HoverStart(id)
                }
                PointerEventKind::Leave { .. } => {
                    self.set_hovered(index, None);
                    Event::HoverEnd(id)
                }
                PointerEventKind::Motion { .. } => {
                    self.hover_action(index, x, y);
                    continue;
                }
                PointerEventKind::Press { button: code, .. } => {
                    let Some(button) = Button::from_code(code) else {
                        continue;
                    };
                    let surface = &self.surfaces[index];
                    // Actions answer the left button; any other acts on the Popup.
                    let action = (button == Button::Left)
                        .then(|| surface.action_rects.iter().position(|r| r.contains(x, y)))
                        .flatten();
                    match action {
                        Some(at) => Event::InvokeAction {
                            id,
                            key: surface.popup.actions[at].key.clone(),
                        },
                        None => Event::Click { id, button },
                    }
                }
                _ => continue,
            };
            self.events.push(event);
        }
    }
}

impl Wayland {
    /// Highlights whichever Action of the Popup at `index` lies under
    /// (`x`, `y`).
    fn hover_action(&mut self, index: usize, x: f64, y: f64) {
        let hovered = self.surfaces[index]
            .action_rects
            .iter()
            .position(|b| b.contains(x, y));
        self.set_hovered(index, hovered);
    }

    /// Redraws the Popup at `index` only when its highlighted Action changes.
    fn set_hovered(&mut self, index: usize, hovered: Option<usize>) {
        let surface = &mut self.surfaces[index];
        if surface.hovered == hovered {
            return;
        }
        surface.hovered = hovered;
        if surface.configured
            && let Err(e) = self.draw(index)
        {
            error!("could not redraw popup: {e}");
        }
    }
}

impl ShmHandler for Wayland {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Wayland {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch2<WpFractionalScaleV1, Wayland> for Scaling {
    fn event(
        &self,
        state: &mut Wayland,
        proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        _: &Connection,
        _: &QueueHandle<Wayland>,
    ) {
        let wp_fractional_scale_v1::Event::PreferredScale { scale } = event else {
            return;
        };
        let surface = state
            .surfaces
            .iter()
            .find(|s| s.fractional.as_ref() == Some(proxy))
            .map(|s| s.layer.wl_surface().clone());
        if let Some(surface) = surface {
            state.rescale(&surface, scale);
        }
    }
}

/// The objects below send no events.
macro_rules! no_events {
    ($($interface:ty),*) => {$(
        impl Dispatch2<$interface, Wayland> for Scaling {
            fn event(
                &self,
                _: &mut Wayland,
                _: &$interface,
                _: <$interface as Proxy>::Event,
                _: &Connection,
                _: &QueueHandle<Wayland>,
            ) {
            }
        }
    )*};
}
no_events!(WpFractionalScaleManagerV1, WpViewporter, WpViewport);

delegate_registry!(Wayland);
delegate_dispatch2!(Wayland);
