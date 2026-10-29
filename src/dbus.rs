//! The D-Bus adapter: serves `org.freedesktop.Notifications` (spec 1.3) with
//! the synchronous `dbus` crate (libdbus), polled from the main event loop
//! (ADR-0003).
//!
//! It turns method calls into core [`Event`]s, answers them, and emits the
//! signals the core asks for. It holds no Notification state of its own.

use std::error::Error;
use std::ffi::CString;
use std::os::fd::RawFd;
use std::sync::Arc;
use std::time::Duration;

use dbus::Message;
use dbus::arg::{PropMap, RefArg, cast};
use dbus::blocking::Connection;
use dbus::blocking::stdintf::org_freedesktop_dbus::RequestNameReply;
use dbus::channel::{BusType, Channel};
use dbus::message::MessageType;

use crate::core::{Action, CloseReason, Effect, Event, Notification, Timeout, Urgency};
use crate::debug;
use crate::image::{Image, Pixels, RawPixels};

pub const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// Stands in for the pointer while drawing is off, so the bus tests can click
/// a Popup that is never shown. Compiled in only with the `test-hooks`
/// feature, and served only after [`DbusAdapter::with_test_hooks`].
#[cfg(feature = "test-hooks")]
const TEST_INTERFACE: &str = "dev.cptdawn.Lucent.Test";

/// Capabilities are added only in the change that implements them.
const CAPABILITIES: [&str; 5] = [
    "actions",
    "body",
    "body-hyperlinks",
    "body-markup",
    "icon-static",
];

/// The session bus went away; nothing more can be served.
#[derive(Debug)]
pub struct Disconnected;

pub struct DbusAdapter {
    conn: Connection,
    #[cfg(feature = "test-hooks")]
    test_hooks: bool,
}

impl DbusAdapter {
    /// Connects to the session bus named by `DBUS_SESSION_BUS_ADDRESS`.
    pub fn connect() -> Result<Self, dbus::Error> {
        let mut channel = Channel::get_private(BusType::Session)?;
        // Must happen before the channel is wrapped, or `watch()` panics.
        channel.set_watch_enabled(true);
        Ok(Self {
            conn: Connection::from(channel),
            #[cfg(feature = "test-hooks")]
            test_hooks: false,
        })
    }

    /// Serves [`TEST_INTERFACE`]: `Click(u id, s button)` with `left`,
    /// `middle` or `right` acts as a pointer press on that Popup, and
    /// `InvokeAction(u id, s key)` as a press on its button for Action `key`.
    #[cfg(feature = "test-hooks")]
    pub fn with_test_hooks(mut self) -> Self {
        self.test_hooks = true;
        self
    }

    /// Claims the well-known name. `Ok(Err(owner))` describes the process that
    /// already holds it.
    pub fn acquire_name(&self) -> Result<Result<(), String>, dbus::Error> {
        match self.conn.request_name(NAME, false, false, true)? {
            RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => Ok(Ok(())),
            _ => Ok(Err(self
                .owner_description()
                .unwrap_or_else(|| "an unknown process".into()))),
        }
    }

    fn owner_description(&self) -> Option<String> {
        let bus = self.conn.with_proxy(
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            Duration::from_secs(1),
        );
        let (pid,): (u32,) = bus
            .method_call(
                "org.freedesktop.DBus",
                "GetConnectionUnixProcessID",
                (NAME,),
            )
            .ok()?;
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(format!("{} (pid {pid})", comm.trim()))
    }

    /// Gives up the well-known name, so the bus activates a fresh Lucent for
    /// the next call. Calls the bus routed here before the release may still
    /// be queued: [`Self::process`] them before exiting.
    pub fn release_name(&self) -> Result<(), dbus::Error> {
        self.conn.release_name(NAME).map(drop)
    }

    /// The connection's fd, to be polled for readability. libdbus owns it.
    pub fn fd(&self) -> RawFd {
        self.conn.channel().watch().fd
    }

    /// Reads and answers every pending message. Each request is turned into an
    /// [`Event`] and passed to `handle`; the effects it returns that the
    /// adapter does not consume itself are handed back for the caller to apply.
    ///
    /// Blocking calls made earlier (such as [`Self::acquire_name`]) may have
    /// queued messages without leaving the fd readable, so call this once
    /// right after startup as well.
    pub fn process(
        &self,
        mut handle: impl FnMut(Event) -> Vec<Effect>,
    ) -> Result<Vec<Effect>, Disconnected> {
        let channel = self.conn.channel();
        channel
            .read_write(Some(Duration::ZERO))
            .map_err(|()| Disconnected)?;
        let mut pending = Vec::new();
        while let Some(msg) = channel.pop_message() {
            if msg.msg_type() != MessageType::MethodCall {
                continue;
            }
            let reply = self.answer(&msg, &mut handle, &mut pending);
            if !msg.get_no_reply() {
                let _ = channel.send(reply);
            }
        }
        channel.flush();
        Ok(pending)
    }

    fn answer(
        &self,
        msg: &Message,
        handle: &mut impl FnMut(Event) -> Vec<Effect>,
        pending: &mut Vec<Effect>,
    ) -> Message {
        let interface = msg.interface();
        let member = msg.member();
        debug!(
            "call {}.{}",
            interface.as_deref().unwrap_or("?"),
            member.as_deref().unwrap_or("?")
        );
        #[cfg(feature = "test-hooks")]
        if self.test_hooks && interface.as_deref() == Some(TEST_INTERFACE) {
            return test_hook(msg, handle, pending);
        }
        if interface.as_deref() != Some(NAME) {
            return unknown_method(msg);
        }
        match member.as_deref() {
            Some("GetCapabilities") => msg.method_return().append1(CAPABILITIES.to_vec()),
            Some("GetServerInformation") => msg
                .method_return()
                .append2("Lucent", "CPT-Dawn")
                .append2(env!("CARGO_PKG_VERSION"), "1.3"),
            Some("Notify") => match parse_notify(msg) {
                Ok(n) => {
                    let mut id = 0;
                    for effect in handle(Event::Notify(n)) {
                        match effect {
                            Effect::Accepted(accepted) => id = accepted,
                            other => pending.push(other),
                        }
                    }
                    msg.method_return().append1(id)
                }
                Err(e) => msg.error(
                    &"org.freedesktop.DBus.Error.InvalidArgs".into(),
                    &CString::new(e.to_string()).unwrap_or_default(),
                ),
            },
            Some("CloseNotification") => match msg.read1::<u32>() {
                Ok(id) => {
                    pending.extend(handle(Event::Close(id)));
                    msg.method_return()
                }
                Err(e) => msg.error(
                    &"org.freedesktop.DBus.Error.InvalidArgs".into(),
                    &CString::new(e.to_string()).unwrap_or_default(),
                ),
            },
            _ => unknown_method(msg),
        }
    }

    /// Queues and sends a `NotificationClosed` signal.
    pub fn emit_closed(&self, id: u32, reason: CloseReason) {
        debug!("NotificationClosed id={id} reason={reason:?}");
        let signal = Message::new_signal(PATH, NAME, "NotificationClosed")
            .expect("static signal names are valid")
            .append2(id, reason as u32);
        let channel = self.conn.channel();
        let _ = channel.send(signal);
        // `send` only queues; signals raised from a timer would otherwise
        // wait for the next incoming message.
        channel.flush();
    }

    /// Queues and sends an `ActionInvoked` signal.
    pub fn emit_action_invoked(&self, id: u32, key: &str) {
        debug!("ActionInvoked id={id} key={key:?}");
        let signal = Message::new_signal(PATH, NAME, "ActionInvoked")
            .expect("static signal names are valid")
            .append2(id, key);
        let channel = self.conn.channel();
        let _ = channel.send(signal);
        channel.flush();
    }
}

fn parse_notify(msg: &Message) -> Result<Notification, Box<dyn Error>> {
    // Signature susssasa{sv}i. The fields not used yet arrive with their tickets.
    let (app_name, replaces_id, app_icon, summary, body, actions, hints, timeout): (
        String,
        u32,
        String,
        String,
        String,
        Vec<String>,
        PropMap,
        i32,
    ) = msg.read_all()?;
    let image = image_from_hints(&app_icon, &hints);
    debug!("Notify from {app_name:?}: {summary:?}, image {image:?}");
    Ok(Notification {
        image,
        app_name,
        summary,
        body,
        replaces_id,
        // A flat list of key, label pairs; a dangling key has no label.
        actions: actions
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[key, label]| Action {
                key: key.clone(),
                label: label.clone(),
            })
            .collect(),
        urgency: urgency(&hints),
        timeout: Timeout::from_millis(timeout),
        // Progress in percent, an int32 by the spec; any integer will do.
        value: hints.get("value").and_then(|v| v.0.as_i64()),
    })
}

#[cfg(feature = "test-hooks")]
fn test_hook(
    msg: &Message,
    handle: &mut impl FnMut(Event) -> Vec<Effect>,
    pending: &mut Vec<Effect>,
) -> Message {
    use crate::core::Button;
    let event = match msg.member().as_deref() {
        Some("Click") => match msg.read2::<u32, &str>() {
            Ok((id, "left")) => Event::Click {
                id,
                button: Button::Left,
            },
            Ok((id, "middle")) => Event::Click {
                id,
                button: Button::Middle,
            },
            Ok((id, "right")) => Event::Click {
                id,
                button: Button::Right,
            },
            _ => {
                return msg.error(
                    &"org.freedesktop.DBus.Error.InvalidArgs".into(),
                    c"expected (u id, s left|middle|right)",
                );
            }
        },
        Some("InvokeAction") => match msg.read2::<u32, String>() {
            Ok((id, key)) => Event::InvokeAction { id, key },
            Err(_) => {
                return msg.error(
                    &"org.freedesktop.DBus.Error.InvalidArgs".into(),
                    c"expected (u id, s key)",
                );
            }
        },
        _ => return unknown_method(msg),
    };
    pending.extend(handle(event));
    msg.method_return()
}

/// The one image a Notification shows, from `app_icon` and its hints.
///
/// The order follows the freedesktop spec (1.3): `image-data`, then
/// `image-path`, then `app_icon`, then the legacy `icon_data`. The spec's
/// older `image_data` / `image_path` spellings count as their dashed names.
/// A hint that cannot be used is skipped in favour of the next one.
pub fn image_from_hints(app_icon: &str, hints: &PropMap) -> Option<Image> {
    let pixels = |key: &str| {
        let value = hints.get(key)?;
        let decoded = decode_pixels(&*value.0);
        if decoded.is_none() {
            debug!("ignoring a malformed {key} hint");
        }
        decoded.map(|p| Image::Pixels(Arc::new(p)))
    };
    let named = |key: &str| hints.get(key)?.0.as_str().and_then(Image::from_name);
    pixels("image-data")
        .or_else(|| pixels("image_data"))
        .or_else(|| named("image-path"))
        .or_else(|| named("image_path"))
        .or_else(|| Image::from_name(app_icon))
        .or_else(|| pixels("icon_data"))
}

/// Unpacks an `(iiibiiay)` hint: width, height, rowstride, has_alpha,
/// bits_per_sample, channels, data.
fn decode_pixels(value: &(dyn RefArg + 'static)) -> Option<Pixels> {
    let int = |i| value.as_static_inner(i)?.as_i64();
    Pixels::from_raw(RawPixels {
        width: int(0)?,
        height: int(1)?,
        rowstride: int(2)?,
        has_alpha: int(3)? != 0,
        bits_per_sample: int(4)?,
        channels: int(5)?,
        data: cast::<Vec<u8>>(value.as_static_inner(6)?)?,
    })
}

/// The `urgency` hint, a byte by the spec; some senders use other integer
/// types, so any integer is accepted.
fn urgency(hints: &PropMap) -> Urgency {
    hints
        .get("urgency")
        .and_then(|v| v.0.as_u64())
        .and_then(|u| u8::try_from(u).ok())
        .map_or(Urgency::Normal, Urgency::from_byte)
}

fn unknown_method(msg: &Message) -> Message {
    msg.error(
        &"org.freedesktop.DBus.Error.UnknownMethod".into(),
        c"Unknown method",
    )
}
