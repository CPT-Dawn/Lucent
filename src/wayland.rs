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

use std::error::Error;

use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::{
    BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind, PointerHandler,
};
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
use wayland_client::{Connection, QueueHandle};

use crate::config::{self, Layout};
use crate::core::{Button, Event, Popup};
use crate::icon::Icons;
use crate::{debug, draw, error, warn};

/// The layer namespace compositor rules can target.
const NAMESPACE: &str = "lucent";

pub struct Wayland {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    pointer: Option<wl_pointer::WlPointer>,
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
    warned_monitor: bool,
    /// In Stack order: the first is closest to the anchor.
    surfaces: Vec<Surface>,
}

struct Surface {
    popup: Popup,
    /// The Popup's image, loaded once at the icon size; dropped with it.
    icon: Option<cairo::ImageSurface>,
    layer: LayerSurface,
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
        let mut wayland = Self {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            seats: SeatState::new(&globals, &qh),
            pointer: None,
            events: Vec::new(),
            compositor,
            layer_shell,
            shm,
            pool,
            qh,
            layout,
            warned_monitor: false,
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
            self.warned_monitor = false;
        } else if !self.warned_monitor {
            let known: Vec<String> = self
                .outputs
                .outputs()
                .filter_map(|o| self.outputs.info(&o).and_then(|i| i.name))
                .collect();
            warn!(
                "no monitor named `{name}` (have: {}); letting the compositor choose",
                known.join(", ")
            );
            self.warned_monitor = true;
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
                surface.icon = load_icon(popup);
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
        let icon = load_icon(&popup);
        let height = draw::measure(&popup, icon.as_ref(), width);
        let output = self.output();
        let surface = self.compositor.create_surface(&self.qh);
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
            width,
            height,
            offset,
            configured: false,
            hovered: None,
        }
    }

    fn draw(&mut self, index: usize) -> Result<(), Box<dyn Error>> {
        let surface = &self.surfaces[index];
        let (w, h) = (surface.width as i32, surface.height as i32);
        let stride = cairo::Format::ARgb32.stride_for_width(surface.width)?;
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
        wl_surface.damage_buffer(0, 0, w, h);
        buffer.attach_to(wl_surface)?;
        surface.layer.commit();
        Ok(())
    }
}

/// The Popup's image at its Theme's icon size, if it has one that loads.
fn load_icon(popup: &Popup) -> Option<cairo::ImageSurface> {
    let image = popup.image.as_ref()?;
    Icons::new(popup.theme.icon_size, &popup.theme.icon_theme).load(image)
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

impl CompositorHandler for Wayland {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
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
                Ok(pointer) => self.pointer = Some(pointer),
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
                PointerEventKind::Enter { .. } => {
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
                PointerEventKind::Press { button, .. } => match button {
                    BTN_LEFT => {
                        let surface = &self.surfaces[index];
                        match surface.action_rects.iter().position(|b| b.contains(x, y)) {
                            Some(at) => Event::InvokeAction {
                                id,
                                key: surface.popup.actions[at].key.clone(),
                            },
                            None => Event::Click {
                                id,
                                button: Button::Left,
                            },
                        }
                    }
                    BTN_MIDDLE => Event::Click {
                        id,
                        button: Button::Middle,
                    },
                    BTN_RIGHT => Event::Click {
                        id,
                        button: Button::Right,
                    },
                    _ => continue,
                },
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

delegate_registry!(Wayland);
delegate_dispatch2!(Wayland);
