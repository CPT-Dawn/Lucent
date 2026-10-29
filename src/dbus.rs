//! The D-Bus adapter: serves `org.freedesktop.Notifications` (spec 1.3) with
//! the synchronous `dbus` crate (libdbus), polled from the main event loop
//! (ADR-0003).
//!
//! It turns method calls into core [`Event`]s, answers them, and emits the
//! signals the core asks for. It holds no Notification state of its own.

use std::error::Error;
use std::ffi::CString;
use std::os::fd::RawFd;
use std::time::Duration;

use dbus::Message;
use dbus::arg::PropMap;
use dbus::blocking::Connection;
use dbus::blocking::stdintf::org_freedesktop_dbus::RequestNameReply;
use dbus::channel::{BusType, Channel};
use dbus::message::MessageType;

use crate::core::{CloseReason, Effect, Event, Notification};
use crate::debug;

pub const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

/// Capabilities are added only in the change that implements them.
const CAPABILITIES: [&str; 2] = ["body", "body-markup"];

/// The session bus went away; nothing more can be served.
#[derive(Debug)]
pub struct Disconnected;

pub struct DbusAdapter {
    conn: Connection,
}

impl DbusAdapter {
    /// Connects to the session bus named by `DBUS_SESSION_BUS_ADDRESS`.
    pub fn connect() -> Result<Self, dbus::Error> {
        let mut channel = Channel::get_private(BusType::Session)?;
        // Must happen before the channel is wrapped, or `watch()` panics.
        channel.set_watch_enabled(true);
        Ok(Self {
            conn: Connection::from(channel),
        })
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
}

fn parse_notify(msg: &Message) -> Result<Notification, Box<dyn Error>> {
    // Signature susssasa{sv}i. The fields not used yet arrive with their tickets.
    let (app_name, _replaces_id, _app_icon, summary, body, _actions, _hints, _timeout): (
        String,
        u32,
        String,
        String,
        String,
        Vec<String>,
        PropMap,
        i32,
    ) = msg.read_all()?;
    debug!("Notify from {app_name:?}: {summary:?}");
    Ok(Notification {
        app_name,
        summary,
        body,
    })
}

fn unknown_method(msg: &Message) -> Message {
    msg.error(
        &"org.freedesktop.DBus.Error.UnknownMethod".into(),
        c"Unknown method",
    )
}
