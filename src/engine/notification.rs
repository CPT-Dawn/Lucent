//! The Notification as the engine sees it: freedesktop fields with every
//! hint already parsed into typed fields by the D-Bus adapter. The engine
//! never sees raw D-Bus variants.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
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
    /// Every Urgency, lowest first.
    pub const ALL: [Urgency; 3] = [Urgency::Low, Urgency::Normal, Urgency::Critical];

    /// Every [`name`](Urgency::name), lowest first.
    pub const NAMES: [&str; 3] = [
        Urgency::Low.name(),
        Urgency::Normal.name(),
        Urgency::Critical.name(),
    ];

    /// The name used in the config, the CLI and `--json`: `low`, `normal`
    /// or `critical`.
    pub const fn name(self) -> &'static str {
        match self {
            Urgency::Low => "low",
            Urgency::Normal => "normal",
            Urgency::Critical => "critical",
        }
    }
}

impl fmt::Display for Urgency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A name that isn't `low`, `normal` or `critical`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownUrgency;

impl FromStr for Urgency {
    type Err = UnknownUrgency;

    /// The exact lowercase [`name`](Urgency::name).
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Urgency::ALL
            .into_iter()
            .find(|u| u.name() == name)
            .ok_or(UnknownUrgency)
    }
}

impl TryFrom<u8> for Urgency {
    type Error = UnknownUrgency;

    /// The freedesktop `urgency` hint byte: 0 low, 1 normal, 2 critical.
    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        match byte {
            0 => Ok(Urgency::Low),
            1 => Ok(Urgency::Normal),
            2 => Ok(Urgency::Critical),
            _ => Err(UnknownUrgency),
        }
    }
}

impl From<Urgency> for u8 {
    /// The freedesktop `urgency` hint byte.
    fn from(urgency: Urgency) -> u8 {
        match urgency {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
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

#[cfg(test)]
mod tests {
    use super::Urgency;

    #[test]
    fn urgency_names_round_trip_and_reject_anything_else() {
        let names: Vec<String> = Urgency::ALL.iter().map(|u| u.to_string()).collect();
        assert_eq!(names, ["low", "normal", "critical"]);
        for urgency in Urgency::ALL {
            assert_eq!(urgency.name().parse(), Ok(urgency));
        }
        assert!(
            "Critical".parse::<Urgency>().is_err(),
            "names are lowercase"
        );
        assert!("urgent".parse::<Urgency>().is_err());
    }

    #[test]
    fn urgency_wire_bytes_round_trip_and_reject_unknown_bytes() {
        assert_eq!(Urgency::try_from(0), Ok(Urgency::Low));
        assert_eq!(Urgency::try_from(1), Ok(Urgency::Normal));
        assert_eq!(Urgency::try_from(2), Ok(Urgency::Critical));
        assert!(Urgency::try_from(3).is_err());
        for urgency in Urgency::ALL {
            assert_eq!(Urgency::try_from(u8::from(urgency)), Ok(urgency));
        }
    }
}
