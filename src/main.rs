//! Lucent — A blazing-fast Wayland notification daemon.
//!
//! Architecture:
//!   1. Bootstrap config from `~/.config/lucent/config.toml`
//!   2. Spawn a tokio thread running the zbus D-Bus server
//!   3. Run the GTK4 main loop, polling the D-Bus→UI channel
//!
//! The two threads communicate via `tokio::sync::mpsc` channels:
//!   - D-Bus → UI:  `UnboundedSender<UiCommand>`  (notification data)
//!   - UI → D-Bus:  `UnboundedSender<DbusSignal>` (signal emissions)

mod config;
mod dbus;
mod notification;
mod ui;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;
use std::time::Duration;

use gtk4::glib;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── 0. Single-instance guard for notification bus name ──────────
    let owner = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(crate::dbus::notifications_name_owner())?;

    if let Some(owner) = owner {
        eprintln!("[lucent] Another notification daemon is already running (owner: {owner}).");
        eprintln!("[lucent] Exiting without error. Stop the existing daemon to run this instance.");
        return Ok(());
    }

    // ── 1. Initialize GTK4 ──────────────────────────────────────────
    gtk4::init()?;

    // ── 2. Load or bootstrap configuration ──────────────────────────
    let config = Arc::new(config::load_or_create_config()?);
    eprintln!(
        "[lucent] v{} starting (width={}px, timeout={}s, max={})",
        env!("CARGO_PKG_VERSION"),
        config.width,
        config.timeout_seconds,
        config.max_visible_notifications,
    );

    // ── 2b. Load CSS theming from config ────────────────────────────
    ui::style::load_css(&config);

    // ── 3. Create inter-thread channels ─────────────────────────────
    //   D-Bus → UI:  notification commands
    let (ui_tx, mut ui_rx) = tokio::sync::mpsc::unbounded_channel::<notification::UiCommand>();

    //   UI → D-Bus:  signal emissions (NotificationClosed, ActionInvoked)
    let (dbus_tx, dbus_rx) = tokio::sync::mpsc::unbounded_channel::<notification::DbusSignal>();

    //   D-Bus thread → UI: fatal startup/runtime errors
    let (dbus_err_tx, dbus_err_rx) = std::sync::mpsc::channel::<String>();

    // Clone ui_tx for State (used by auto-dismiss timers & click handlers).
    let ui_tx_for_state = ui_tx.clone();

    // ── 4. Spawn D-Bus server on a dedicated tokio thread ───────────
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                let _ = dbus_err_tx.send(format!("Failed to create tokio runtime: {e}"));
                return;
            }
        };

        rt.block_on(async move {
            if let Err(e) = crate::dbus::run_server(ui_tx, dbus_rx).await {
                let _ = dbus_err_tx.send(format!("D-Bus server error: {e}"));
            }
        });
    });

    // ── 5. Set up UI state on the GTK main thread ───────────────────
    let state = Rc::new(RefCell::new(ui::State::new(
        config,
        dbus_tx,
        ui_tx_for_state,
    )));

    let main_loop = glib::MainLoop::new(None, false);

    // Poll the D-Bus → UI channel from within the GTK main loop.
    // 50 ms cadence is imperceptible for notification popups and avoids
    // busy-waiting while staying responsive.
    {
        let state = state.clone();
        let main_loop = main_loop.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            while let Ok(cmd) = ui_rx.try_recv() {
                state.borrow_mut().handle_command(cmd);
            }

            match dbus_err_rx.try_recv() {
                Ok(err) => {
                    eprintln!("[lucent] {err}");
                    eprintln!("[lucent] Shutting down due to D-Bus failure.");
                    main_loop.quit();
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Disconnected) => {
                    eprintln!("[lucent] D-Bus thread terminated unexpectedly.");
                    eprintln!("[lucent] Shutting down.");
                    main_loop.quit();
                    glib::ControlFlow::Break
                }
                Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            }
        });
    }

    // ── 6. Run the GTK main loop ────────────────────────────────────
    eprintln!("[lucent] Listening for notifications on D-Bus…");
    main_loop.run();

    Ok(())
}
