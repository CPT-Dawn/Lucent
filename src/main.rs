//! Lucent, a Wayland notification daemon.
//!
//! The behaviour lives in the GTK-free, D-Bus-free [`lucent::engine`]. This
//! binary wires it to its adapters:
//!
//! - the D-Bus adapter ([`dbus`]) runs zbus on its own thread and sends
//!   engine events over a channel;
//! - the GLib main loop awaits that channel (no polling), feeds the engine
//!   and hands its effects to the GTK adapter ([`ui`]), the timer adapter
//!   ([`timers`]) and back to the D-Bus thread for signals.

mod config;
mod dbus;
mod timers;
mod ui;

use std::rc::Rc;

use gtk4::glib;
use lucent::engine::{Effect, Engine, Event, IdAllocator, SystemClock};
use tokio::sync::{mpsc, oneshot};
use tracing_subscriber::EnvFilter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("lucent=info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let owner = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(dbus::notifications_name_owner())?;
    if let Some(owner) = owner {
        tracing::warn!(
            "another notification daemon ({owner}) owns org.freedesktop.Notifications; \
             stop it to run Lucent"
        );
        return Ok(());
    }

    gtk4::init()?;

    let config = Rc::new(config::load_or_create_config()?);
    tracing::info!(
        "v{} starting (width={}px, timeout={}s, max={})",
        env!("CARGO_PKG_VERSION"),
        config.width,
        config.timeout_seconds,
        config.max_visible_notifications,
    );
    ui::style::load_css(&config);

    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<Event>();
    let (signals_tx, signals_rx) = mpsc::unbounded_channel::<Effect>();
    let (fatal_tx, fatal_rx) = oneshot::channel::<String>();

    let ids = IdAllocator::new();
    {
        let events_tx = events_tx.clone();
        let ids = ids.clone();
        std::thread::Builder::new()
            .name("lucent-dbus".into())
            .spawn(move || {
                let reason = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => match rt.block_on(dbus::run_server(events_tx, ids, signals_rx)) {
                        Ok(()) => "D-Bus signal channel closed".to_owned(),
                        Err(e) => format!("D-Bus server error: {e}"),
                    },
                    Err(e) => format!("failed to create tokio runtime: {e}"),
                };
                let _ = fatal_tx.send(reason);
            })?;
    }

    let mut engine = Engine::new(config.engine_config(), Box::new(SystemClock));
    let ui = ui::Ui::new(config.clone(), events_tx.clone());
    let timers = timers::Timers::new(events_tx);
    let main_loop = glib::MainLoop::new(None, false);

    glib::spawn_future_local(async move {
        while let Some(event) = events_rx.recv().await {
            for effect in engine.handle(event) {
                ui.apply(&effect);
                timers.apply(&effect);
                if dbus::handles(&effect) {
                    let _ = signals_tx.send(effect);
                }
            }
        }
    });

    {
        let main_loop = main_loop.clone();
        glib::spawn_future_local(async move {
            let reason = fatal_rx
                .await
                .unwrap_or_else(|_| "D-Bus thread terminated unexpectedly".to_owned());
            tracing::error!("{reason}; shutting down");
            main_loop.quit();
        });
    }

    tracing::info!("listening for Notifications on D-Bus");
    main_loop.run();
    Ok(())
}
