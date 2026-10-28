//! The Notification as the engine sees it: freedesktop fields with every
//! hint already parsed into typed fields by the D-Bus adapter. The engine
//! never sees raw D-Bus variants.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// A Notification id. Never 0; a later Notification may reuse an id to
/// replace the one that carries it.
pub type NotificationId = u32;

/// A message an app sends to Lucent.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: NotificationId,
    pub app_name: String,
    /// Icon name or path from the `app_icon` argument (may be empty).
    pub app_icon: String,
    pub summary: String,
    /// Body text, possibly with Pango-style markup (may be empty).
    pub body: String,
    pub actions: Vec<Action>,
    pub hints: Hints,
    pub expiry: Expiry,
}

/// An app-defined choice attached to a Notification. The key `"default"`
/// is the Action invoked by clicking the Popup body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// How long the app asked for the Notification to stay, from the
/// `expire_timeout` argument of `Notify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Expiry {
    /// `-1`: let Lucent decide (the Urgency default).
    #[default]
    Default,
    /// `0`: never expire.
    Never,
    /// `> 0`: expire after this long. Always wins over Lucent's defaults.
    After(Duration),
}

impl Expiry {
    /// Interpret the raw `expire_timeout` milliseconds from `Notify`.
    pub fn from_millis(expire_timeout: i32) -> Self {
        match expire_timeout {
            0 => Expiry::Never,
            ms if ms > 0 => Expiry::After(Duration::from_millis(ms as u64)),
            _ => Expiry::Default,
        }
    }
}

/// How important a Notification is, as set by the sending app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    /// Interpret the `urgency` hint byte (0 low, 1 normal, 2 critical).
    /// Unknown values fall back to normal.
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Urgency::Low,
            2 => Urgency::Critical,
            _ => Urgency::Normal,
        }
    }
}

/// The freedesktop hints Lucent understands, parsed at the D-Bus edge.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Hints {
    pub urgency: Urgency,
    /// `image-data` (preferred) or `image-path`.
    pub image: Option<Image>,
    /// The `value` hint: progress percentage, clamped to 0..=100.
    pub progress: Option<u8>,
    /// The app marked this Notification as not worth remembering.
    pub transient: bool,
    pub desktop_entry: Option<String>,
    pub category: Option<String>,
}

/// An image attached to a Notification.
#[derive(Debug, Clone, PartialEq)]
pub enum Image {
    /// An icon name or file path from `image-path`.
    Path(PathBuf),
    /// Raw pixels from `image-data`.
    Data(ImageData),
}

/// Raw pixel data from the `image-data` hint, in the layout the spec
/// defines (`(iiibiiay)`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    pub width: i32,
    pub height: i32,
    pub rowstride: i32,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub channels: i32,
    /// Shared so Popups and History can hold the same pixels cheaply.
    pub data: Arc<[u8]>,
}
