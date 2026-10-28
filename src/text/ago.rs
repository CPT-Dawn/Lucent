//! Relative times: "now", "12s", "3m", "2h", "1d" on a Popup, and the
//! same with "ago" after it ("just now", "3m ago") in `lucent history`.

use std::time::Duration;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

/// Below this, the time reads "now".
const NOW: u64 = 10;

/// How long ago, as shown in the Popup header.
pub fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() {
        s if s < NOW => "now".to_owned(),
        s if s < MINUTE => format!("{s}s"),
        s if s < HOUR => format!("{}m", s / MINUTE),
        s if s < DAY => format!("{}h", s / HOUR),
        s => format!("{}d", s / DAY),
    }
}

/// How long ago, for a sentence: "just now", or [`ago`] with " ago".
pub fn ago_phrase(elapsed: Duration) -> String {
    if elapsed.as_secs() < NOW {
        "just now".to_owned()
    } else {
        format!("{} ago", ago(elapsed))
    }
}

/// How long until [`ago`] shows something different.
pub fn until_change(elapsed: Duration) -> Duration {
    let next = match elapsed.as_secs() {
        s if s < NOW => NOW,
        s if s < MINUTE => s + 1,
        s if s < HOUR => (s / MINUTE + 1) * MINUTE,
        s if s < DAY => (s / HOUR + 1) * HOUR,
        s => (s / DAY + 1) * DAY,
    };
    Duration::from_secs(next) - elapsed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: f64) -> Duration {
        Duration::from_secs_f64(s)
    }

    #[test]
    fn reads_like_the_prototype() {
        assert_eq!(ago(secs(0.0)), "now");
        assert_eq!(ago(secs(9.9)), "now");
        assert_eq!(ago(secs(12.0)), "12s");
        assert_eq!(ago(secs(59.5)), "59s");
        assert_eq!(ago(secs(60.0)), "1m");
        assert_eq!(ago(secs(185.0)), "3m");
    }

    #[test]
    fn older_times_count_in_hours_then_days() {
        assert_eq!(ago(secs(3_599.0)), "59m");
        assert_eq!(ago(secs(3_600.0)), "1h");
        assert_eq!(ago(secs(3.0 * 3_600.0 + 59.0 * 60.0)), "3h");
        assert_eq!(ago(secs(86_400.0)), "1d");
        assert_eq!(ago(secs(2.5 * 86_400.0)), "2d");
    }

    #[test]
    fn the_cli_phrase_is_the_same_time_with_ago_after_it() {
        assert_eq!(ago_phrase(secs(3.0)), "just now");
        assert_eq!(ago_phrase(secs(20.0)), "20s ago");
        assert_eq!(ago_phrase(secs(300.0)), "5m ago");
        assert_eq!(ago_phrase(secs(3.0 * 3_600.0)), "3h ago");
        assert_eq!(ago_phrase(secs(2.0 * 86_400.0)), "2d ago");
    }

    #[test]
    fn wakes_up_only_when_the_text_changes() {
        assert_eq!(until_change(secs(2.0)), secs(8.0));
        assert_eq!(until_change(secs(12.25)), secs(0.75));
        assert_eq!(until_change(secs(90.0)), secs(30.0));
        assert_eq!(until_change(secs(3_540.0)), secs(60.0));
        assert_eq!(until_change(secs(3_600.0 + 30.0)), secs(3_570.0));
        assert_eq!(until_change(secs(86_400.0)), secs(86_400.0));
    }
}
