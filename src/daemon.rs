//! The notification daemon (`lucent` with no subcommand).
//!
//! The behaviour lives in the GTK-free, D-Bus-free [`lucent::engine`]. This
//! wires it to its adapters:
//!
//! - the D-Bus adapter ([`dbus`]) runs zbus on its own thread and sends
//!   engine events and control requests over channels;
//! - the GLib main loop awaits those channels (no polling), feeds the
//!   engine and hands its effects to the GTK adapter ([`ui`]), the timer
//!   adapter ([`timers`]) and back to the D-Bus thread for signals;
//! - [`live_config`] reloads the config whenever it is saved.

use std::rc::Rc;

use gtk4::glib;
use lucent::config::file;
use lucent::engine::{Effect, Engine, Event, IdAllocator, SystemClock};
use tokio::sync::{mpsc, oneshot};

use crate::dbus::control::Request;
use crate::engine_host::Core;
use crate::live_config::LiveConfig;
use crate::{dbus, timers, ui};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
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

    let path = file::path().ok_or("can't find the config directory (is $HOME set?)")?;
    let style_path = file::style_path(&path);
    let (config, live_config) = LiveConfig::load(path);
    tracing::info!(
        "v{} starting (width={}px, max_visible={})",
        env!("CARGO_PKG_VERSION"),
        config.popup.width,
        config.popup.max_visible,
    );
    let stylesheet = Rc::new(ui::style::Stylesheet::new(&config, style_path));

    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<Event>();
    let (requests_tx, mut requests_rx) = mpsc::unbounded_channel::<Request>();
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
                    Ok(rt) => {
                        match rt.block_on(dbus::run_server(events_tx, ids, requests_tx, signals_rx))
                        {
                            Ok(()) => "D-Bus signal channel closed".to_owned(),
                            Err(e) => format!("D-Bus server error: {e}"),
                        }
                    }
                    Err(e) => format!("failed to create tokio runtime: {e}"),
                };
                let _ = fatal_tx.send(reason);
            })?;
    }

    let engine = Engine::new(config.engine_config(), Box::new(SystemClock)).with_ids(ids);
    let ui = Rc::new(ui::Ui::new(Rc::new(config), events_tx.clone()));
    ui.set_theme(stylesheet.theme());
    {
        let ui = ui.clone();
        stylesheet.connect_theme_changed(move |theme| ui.set_theme(theme.clone()));
    }
    let timers = timers::Timers::new(events_tx.clone());
    let core = Rc::new(Core::new(engine, ui.clone(), timers, signals_tx));
    let main_loop = glib::MainLoop::new(None, false);

    let live_config = {
        let ui = ui.clone();
        let restyle = stylesheet.clone();
        live_config
            .on_style_change(move || restyle.reload_user_css())
            .watch(move |config| {
                stylesheet.apply(config);
                ui.replace_config(Rc::new(config.clone()));
                let _ = events_tx.send(Event::ConfigReplaced(config.engine_config()));
            })
    };

    {
        let core = core.clone();
        glib::spawn_future_local(async move {
            while let Some(event) = events_rx.recv().await {
                core.handle(event);
            }
        });
    }

    glib::spawn_future_local(async move {
        while let Some(request) = requests_rx.recv().await {
            match request {
                Request::ReloadConfig(reply) => {
                    let _ = reply.send(live_config.reload());
                }
                Request::Engine(command) => core.answer(command),
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
