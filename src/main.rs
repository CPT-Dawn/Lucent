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
use std::sync::Arc;
use std::time::Duration;

use gtk4::glib;

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    // Clone ui_tx for State (used by auto-dismiss timers & click handlers).
    let ui_tx_for_state = ui_tx.clone();

    // ── 4. Spawn D-Bus server on a dedicated tokio thread ───────────
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");

        rt.block_on(async move {
            if let Err(e) = crate::dbus::run_server(ui_tx, dbus_rx).await {
                eprintln!("[lucent] D-Bus server error: {e}");
                std::process::exit(1);
            }
        });
    });

    // ── 5. Set up UI state on the GTK main thread ───────────────────
    let state = Rc::new(RefCell::new(ui::State::new(
        config,
        dbus_tx,
        ui_tx_for_state,
    )));

    // Poll the D-Bus → UI channel from within the GTK main loop.
    // 50 ms cadence is imperceptible for notification popups and avoids
    // busy-waiting while staying responsive.
    {
        let state = state.clone();
        glib::timeout_add_local(Duration::from_millis(50), move || {
            while let Ok(cmd) = ui_rx.try_recv() {
                state.borrow_mut().handle_command(cmd);
            }
            glib::ControlFlow::Continue
        });
    }

    // ── 6. Run the GTK main loop ────────────────────────────────────
    eprintln!("[lucent] Listening for notifications on D-Bus…");
    let main_loop = glib::MainLoop::new(None, false);
    main_loop.run();

    Ok(())
}
