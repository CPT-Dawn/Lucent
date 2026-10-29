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
use crate::image::{Image, Pixels, RawPixels};
use crate::{debug, error};

pub const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// Stands in for the pointer while drawing is off, so the bus tests can click
/// a Popup that is never shown. Compiled in only with the `test-hooks`
/// feature, and served only after [`DbusAdapter::enable_test_hooks`].
#[cfg(feature = "test-hooks")]
const TEST_INTERFACE: &str = "dev.cptdawn.Lucent.Test";

/// Capabilities are added only in the change that implements them. Not
/// `body-hyperlinks`: links are styled but cannot be clicked, and a sender
/// told they can hides the address behind the link text.
const CAPABILITIES: [&str; 4] = ["actions", "body", "body-markup", "icon-static"];

/// How long a handed-off Notify may take, D-Bus activation of the new
/// Lucent included: libdbus's own default call timeout.
const HAND_OFF_TIMEOUT: Duration = Duration::from_secs(25);

/// The session bus went away; nothing more can be served.
#[derive(Debug)]
pub struct Disconnected;

pub struct DbusAdapter {
    conn: Connection,
    /// Whether this process holds [`NAME`]. Once it has let go, a Notify
    /// still routed here is handed to the new owner instead of shown here.
    owns_name: bool,
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
            owns_name: false,
            #[cfg(feature = "test-hooks")]
            test_hooks: false,
        })
    }

    /// Serves [`TEST_INTERFACE`]: `Click(u id, s button)` with `left`,
    /// `middle` or `right` acts as a pointer press on that Popup, and
    /// `InvokeAction(u id, s key)` as a press on its Action `key`.
    #[cfg(feature = "test-hooks")]
    pub fn enable_test_hooks(&mut self) {
        self.test_hooks = true;
    }

    /// Claims the well-known name. `Ok(Err(owner))` describes the process that
    /// already holds it.
    pub fn acquire_name(&mut self) -> Result<Result<(), String>, dbus::Error> {
        match self.conn.request_name(NAME, false, false, true)? {
            RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => {
                self.owns_name = true;
                Ok(Ok(()))
            }
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
    /// be queued: [`Self::process`] them before exiting. Any Notify among
    /// them is handed to the new owner, never shown by this process.
    pub fn release_name(&mut self) -> Result<(), dbus::Error> {
        self.conn.release_name(NAME)?;
        self.owns_name = false;
        Ok(())
    }

    /// The connection's fd, to be polled for readability. libdbus owns it.
    pub fn fd(&self) -> RawFd {
        self.conn.channel().watch().fd
    }

    /// Reads and answers every queued message. Each request is turned into an
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
        let mut to_apply = Vec::new();
        while let Some(msg) = channel.pop_message() {
            if msg.msg_type() != MessageType::MethodCall {
                continue;
            }
            let reply = self.answer(&msg, &mut handle, &mut to_apply);
            if !msg.get_no_reply() {
                let _ = channel.send(reply);
            }
        }
        channel.flush();
        Ok(to_apply)
    }

    fn answer(
        &self,
        msg: &Message,
        handle: &mut impl FnMut(Event) -> Vec<Effect>,
        to_apply: &mut Vec<Effect>,
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
            return test_hook(msg, handle, to_apply);
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
            Some("Notify") if !self.owns_name => self.hand_off(msg),
            Some("Notify") => match parse_notify(msg) {
                Ok(n) => {
                    let mut id = 0;
                    for effect in handle(Event::Notify(n)) {
                        match effect {
                            Effect::Accepted(accepted) => id = accepted,
                            other => to_apply.push(other),
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
                    to_apply.extend(handle(Event::Close(id)));
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

    /// Passes a Notify that reached this process after it let go of the name
    /// on to the name's new owner (D-Bus starts one if need be), and answers
    /// with that owner's reply. The Notification then lives where its id was
    /// handed out and its signals come from the name owner, as clients expect.
    fn hand_off(&self, msg: &Message) -> Message {
        debug!("handing a late Notify to the new owner of {NAME}");
        let forwarded = Message::new_method_call(NAME, PATH, NAME, "Notify")
            .map(|mut m| {
                m.append_items(&msg.get_items());
                m
            })
            .map_err(|e| dbus::Error::new_failed(&e));
        let reply = forwarded.and_then(|m| {
            self.conn
                .channel()
                .send_with_reply_and_block(m, HAND_OFF_TIMEOUT)
        });
        match reply.and_then(|r| r.read1::<u32>().map_err(dbus::Error::from)) {
            Ok(id) => msg.method_return().append1(id),
            Err(e) => {
                error!("could not hand a Notify to the new {NAME}: {e}");
                msg.error(
                    &"org.freedesktop.DBus.Error.Failed".into(),
                    &CString::new(e.to_string()).unwrap_or_default(),
                )
            }
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

    /// Queues and sends an `ActivationToken` signal (spec 1.2), which must
    /// come just before the `ActionInvoked` it belongs to.
    pub fn emit_activation_token(&self, id: u32, token: &str) {
        debug!("ActivationToken id={id}");
        let signal = Message::new_signal(PATH, NAME, "ActivationToken")
            .expect("static signal names are valid")
            .append2(id, token);
        let _ = self.conn.channel().send(signal);
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
        value: hints.get("value").and_then(|v| integer(&*v.0)),
    })
}

#[cfg(feature = "test-hooks")]
fn test_hook(
    msg: &Message,
    handle: &mut impl FnMut(Event) -> Vec<Effect>,
    to_apply: &mut Vec<Effect>,
) -> Message {
    use crate::core::Button;
    let event = match msg.member().as_deref() {
        Some("Click") => {
            let click = msg.read2::<u32, &str>().ok();
            match click.and_then(|(id, name)| Some((id, Button::from_name(name)?))) {
                Some((id, button)) => Event::Click { id, button },
                None => {
                    return msg.error(
                        &"org.freedesktop.DBus.Error.InvalidArgs".into(),
                        c"expected (u id, s left|middle|right)",
                    );
                }
            }
        }
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
    to_apply.extend(handle(event));
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
        .and_then(|v| integer(&*v.0))
        .and_then(|u| u8::try_from(u).ok())
        .map_or(Urgency::Normal, Urgency::from_byte)
}

/// Any D-Bus integer, signed or not, that fits an `i64`. The `dbus` crate's
/// `as_i64` turns down `u64` and its `as_u64` every signed type, so each
/// alone misses some senders.
fn integer(value: &(dyn RefArg + 'static)) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|u| i64::try_from(u).ok()))
}

fn unknown_method(msg: &Message) -> Message {
    msg.error(
        &"org.freedesktop.DBus.Error.UnknownMethod".into(),
        c"Unknown method",
    )
}
