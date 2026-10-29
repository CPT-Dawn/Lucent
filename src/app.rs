//! Wires the core to its adapters on a single `calloop` event loop.
//!
//! Lucent itself spawns no threads and runs no async runtime (ADR-0003): the
//! D-Bus fd, the Wayland connection and the expiry timers are all sources on
//! this one loop, which blocks until one of them is ready.
//!
//! Pango starts its own idle `[pango] fontconfig` helper thread the first
//! time it lays out text, and offers no way to disable it. That thread is
//! Pango's, not Lucent's; with drawing off (`LUCENT_NO_DRAW`) it never starts.

use std::collections::HashMap;
use std::error::Error;
use std::os::fd::{AsFd, BorrowedFd, RawFd};

use calloop::generic::Generic;
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction, RegistrationToken};

use crate::config::Config;
use crate::core::{Core, Effect, Event};
use crate::dbus::{self, DbusAdapter};
use crate::ids::IdStore;
use crate::wayland::Wayland;
use crate::{debug, error};

/// What the process should do once [`run`] returns.
pub enum Exit {
    /// Ran until there was nothing left to serve.
    Done,
    /// Another daemon owns the bus name; it has been named in the log.
    NameTaken,
}

struct App {
    core: Core,
    dbus: DbusAdapter,
    /// `None` when drawing is off: the core and D-Bus still run unchanged.
    wayland: Option<Wayland>,
    handle: LoopHandle<'static, App>,
    timers: HashMap<u32, RegistrationToken>,
    idle_timer: Option<RegistrationToken>,
    ids: IdStore,
    /// The last id written to `ids`.
    saved_id: u32,
    exit: bool,
}

/// The libdbus connection fd, borrowed for polling. libdbus owns and closes it.
struct DbusFd(RawFd);

impl AsFd for DbusFd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: the fd belongs to the connection held by `App`, which
        // outlives every event source on the loop.
        unsafe { BorrowedFd::borrow_raw(self.0) }
    }
}

/// Set (to anything) to run without a compositor, as the bus tests do.
const NO_DRAW: &str = "LUCENT_NO_DRAW";

pub fn run(config: Config) -> Result<Exit, Box<dyn Error>> {
    let no_draw = std::env::var_os(NO_DRAW).is_some();
    let mut dbus = DbusAdapter::connect()?;
    // No pointer without a compositor: let the bus tests click instead.
    #[cfg(feature = "test-hooks")]
    if no_draw {
        dbus.enable_test_hooks();
    }
    if let Err(owner) = dbus.acquire_name()? {
        error!(
            "{} is already owned by {owner}; stop it to use Lucent",
            dbus::NAME
        );
        return Ok(Exit::NameTaken);
    }
    debug!("acquired {}", dbus::NAME);

    let mut event_loop: EventLoop<'static, App> = EventLoop::try_new()?;
    let handle = event_loop.handle();
    handle.insert_source(
        Generic::new(DbusFd(dbus.fd()), Interest::READ, Mode::Level),
        |_, _, app: &mut App| {
            app.serve_dbus();
            Ok(if app.exit {
                PostAction::Remove
            } else {
                PostAction::Continue
            })
        },
    )?;

    let wayland = if no_draw {
        debug!("{NO_DRAW} is set; drawing is off");
        None
    } else {
        let (wayland, source) = Wayland::connect(config.layout.clone())?;
        handle.insert_source(source, |(), queue, app: &mut App| {
            let Some(wayland) = app.wayland.as_mut() else {
                return Ok(0);
            };
            let dispatched = queue.dispatch_pending(wayland)?;
            for event in wayland.take_events() {
                let effects = app.core.handle(event);
                app.apply(effects);
            }
            Ok(dispatched)
        })?;
        Some(wayland)
    };

    let ids = IdStore::open();
    let saved_id = ids.load();
    let mut app = App {
        core: Core::with_config(config).resuming_after(saved_id),
        dbus,
        wayland,
        handle,
        timers: HashMap::new(),
        idle_timer: None,
        ids,
        saved_id,
        exit: false,
    };
    let effects = app.core.handle(Event::Started);
    app.apply(effects);
    // `acquire_name` blocked on the bus and may already have queued requests.
    app.serve_dbus();

    while !app.exit {
        // No timeout: sleep until a source is ready, so idle means no wakeups.
        event_loop.dispatch(None, &mut app)?;
    }
    Ok(Exit::Done)
}

/// With the `test-hooks` feature, sleeps for `LUCENT_TEST_EXIT_PAUSE_MS`, so
/// the bus tests can send a Notify on either side of the name's release.
fn test_exit_pause() {
    #[cfg(feature = "test-hooks")]
    if let Some(ms) = std::env::var("LUCENT_TEST_EXIT_PAUSE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

impl App {
    fn serve_dbus(&mut self) {
        let Self {
            core,
            ids,
            saved_id,
            ..
        } = self;
        let processed = self.dbus.process(|event| {
            let effects = core.handle(event);
            // Saved before the reply leaves, so a Lucent started after this
            // one, by Idle exit or by a crash, never reuses an id.
            if core.last_id() != *saved_id {
                *saved_id = core.last_id();
                ids.save(*saved_id);
            }
            effects
        });
        match processed {
            Ok(effects) => self.apply(effects),
            Err(dbus::Disconnected) => {
                error!("lost the session bus connection");
                self.exit = true;
            }
        }
    }

    /// Releases the bus name, answers what was already routed here, and
    /// exits.
    ///
    /// A Notify sent before the release can still be waiting. It is handed
    /// to the name's next owner rather than shown here, so no Notification
    /// is lost, none lives in a process whose signals clients ignore, and
    /// only the name owner ever hands out ids (ADR-0002).
    fn idle_exit(&mut self) {
        test_exit_pause();
        let released = self.dbus.release_name();
        test_exit_pause();
        self.serve_dbus();
        match released {
            Ok(()) => {
                debug!("idle; exiting");
                self.exit = true;
            }
            // Still the owner, so anything late was shown here: stay, and
            // try again once idle.
            Err(e) => error!("could not release {}, staying: {e}", dbus::NAME),
        }
    }

    fn apply(&mut self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                // Answered by the D-Bus adapter before effects reach here.
                Effect::Accepted(_) => {}
                Effect::Stack(stack) => {
                    debug!("stack now holds {} popup(s)", stack.len());
                    if let Some(wayland) = self.wayland.as_mut() {
                        wayland.show(&stack);
                    }
                }
                Effect::StartTimer { id, after } => {
                    let token = self.handle.insert_source(
                        Timer::from_duration(after),
                        move |_, _, app: &mut App| {
                            app.timers.remove(&id);
                            let effects = app.core.handle(Event::TimerFired(id));
                            app.apply(effects);
                            TimeoutAction::Drop
                        },
                    );
                    match token {
                        Ok(token) => {
                            if let Some(old) = self.timers.insert(id, token) {
                                self.handle.remove(old);
                            }
                        }
                        Err(e) => error!("could not start the timer for {id}: {e}"),
                    }
                }
                Effect::CancelTimer(id) => {
                    if let Some(token) = self.timers.remove(&id) {
                        self.handle.remove(token);
                    }
                }
                Effect::Closed { id, reason } => self.dbus.emit_closed(id, reason),
                Effect::StartIdleTimer { after } => {
                    let token = self.handle.insert_source(
                        Timer::from_duration(after),
                        |_, _, app: &mut App| {
                            app.idle_timer = None;
                            let effects = app.core.handle(Event::IdleTimerFired);
                            app.apply(effects);
                            TimeoutAction::Drop
                        },
                    );
                    match token {
                        Ok(token) => {
                            if let Some(old) = self.idle_timer.replace(token) {
                                self.handle.remove(old);
                            }
                        }
                        Err(e) => error!("could not start the idle timer: {e}"),
                    }
                }
                Effect::CancelIdleTimer => {
                    if let Some(token) = self.idle_timer.take() {
                        self.handle.remove(token);
                    }
                }
                Effect::Exit => self.idle_exit(),
                Effect::ActionInvoked { id, key } => self.dbus.emit_action_invoked(id, &key),
            }
        }
    }
}
