//! The relative time on a Popup ("now", "12s", "3m").

use std::time::Duration;

/// How long ago, as shown in the Popup header.
pub fn ago(elapsed: Duration) -> String {
    match elapsed.as_secs() {
        s if s < 10 => "now".to_owned(),
        s if s < 60 => format!("{s}s"),
        s => format!("{}m", s / 60),
    }
}

/// How long until [`ago`] shows something different.
pub fn until_change(elapsed: Duration) -> Duration {
    let secs = elapsed.as_secs();
    let next = match secs {
        s if s < 10 => 10,
        s if s < 60 => s + 1,
        s => (s / 60 + 1) * 60,
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
    fn wakes_up_only_when_the_text_changes() {
        assert_eq!(until_change(secs(2.0)), secs(8.0));
        assert_eq!(until_change(secs(12.25)), secs(0.75));
        assert_eq!(until_change(secs(90.0)), secs(30.0));
    }
}
