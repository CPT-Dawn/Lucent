//! The Wayland renderer: keeps one layer-shell surface per Popup in step with
//! the Stack the core asks for, and fills each from a `wl_shm` buffer.
//!
//! A Popup is drawn once per configure, never per frame; animation is the
//! compositor's job (ADR-0001).

use std::error::Error;

use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::core::Popup;
use crate::{debug, draw, error};

/// The layer namespace compositor rules can target.
const NAMESPACE: &str = "lucent";
/// Distance from the anchor corner: top, right, bottom, left.
const MARGIN: (i32, i32, i32, i32) = (16, 16, 0, 0);
/// Space between neighbouring Popups in the Stack.
const GAP: i32 = 8;

pub struct Wayland {
    registry: RegistryState,
    outputs: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    pool: SlotPool,
    qh: QueueHandle<Wayland>,
    surfaces: Vec<Surface>,
}

struct Surface {
    popup: Popup,
    layer: LayerSurface,
    width: u32,
    height: u32,
    /// Distance from the anchor edge, including the outer margin.
    offset: i32,
    /// Whether the first configure has arrived, so drawing is allowed.
    configured: bool,
}

impl Wayland {
    /// Connects to the compositor named by `WAYLAND_DISPLAY`. The returned
    /// source must be inserted into the event loop, dispatching into this
    /// `Wayland`.
    pub fn connect() -> Result<(Self, WaylandSource<Wayland>), Box<dyn Error>> {
        let conn = Connection::connect_to_env()?;
        let (globals, queue) = registry_queue_init::<Wayland>(&conn)?;
        let qh = queue.handle();
        let compositor = CompositorState::bind(&globals, &qh)?;
        let layer_shell = LayerShell::bind(&globals, &qh)
            .map_err(|e| format!("the compositor has no layer-shell support: {e}"))?;
        let shm = Shm::bind(&globals, &qh)?;
        let pool = SlotPool::new((draw::WIDTH * 120 * 4) as usize, &shm)?;
        let wayland = Self {
            registry: RegistryState::new(&globals),
            outputs: OutputState::new(&globals, &qh),
            compositor,
            layer_shell,
            shm,
            pool,
            qh,
            surfaces: Vec::new(),
        };
        Ok((wayland, WaylandSource::new(conn, queue)))
    }

    /// Makes the on-screen Popups match `stack`, nearest the anchor first.
    /// Popups whose content changed are redrawn in place; the rest are only
    /// moved, and only if their position changed.
    pub fn show(&mut self, stack: &[Popup]) {
        // Dropping a LayerSurface destroys it.
        self.surfaces
            .retain(|s| stack.iter().any(|p| p.id == s.popup.id));
        let mut offset = MARGIN.0;
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
            offset += self.surfaces[index].height as i32 + GAP;
        }
    }

    /// Brings an open Popup up to date with `popup` at `offset`.
    fn update(&mut self, index: usize, popup: &Popup, offset: i32) {
        let surface = &mut self.surfaces[index];
        let mut commit = false;
        let mut redraw = false;
        if surface.popup != *popup {
            surface.popup = popup.clone();
            let height = draw::measure(popup, surface.width);
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
            set_margin(&surface.layer, offset);
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
        let (width, height) = (draw::WIDTH, draw::measure(&popup, draw::WIDTH));
        let surface = self.compositor.create_surface(&self.qh);
        // Output None: the compositor picks, normally the focused monitor.
        let layer = self.layer_shell.create_layer_surface(
            &self.qh,
            surface,
            Layer::Overlay,
            Some(NAMESPACE),
            None,
        );
        layer.set_anchor(Anchor::TOP | Anchor::RIGHT);
        set_margin(&layer, offset);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(width, height);
        // The first commit carries no buffer; drawing waits for configure.
        layer.commit();
        debug!("opened popup {} ({width}x{height})", popup.id);
        Surface {
            popup,
            layer,
            width,
            height,
            offset,
            configured: false,
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
            let painted = draw::paint(&cr, &surface.popup, surface.width, surface.height);
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

/// Places a Popup `offset` pixels from the anchor edge.
fn set_margin(layer: &LayerSurface, offset: i32) {
    let (_, right, bottom, left) = MARGIN;
    layer.set_margin(offset, right, bottom, left);
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
        if configure.new_size.0 != 0 {
            surface.width = configure.new_size.0;
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

impl ShmHandler for Wayland {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Wayland {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_registry!(Wayland);
delegate_dispatch2!(Wayland);
