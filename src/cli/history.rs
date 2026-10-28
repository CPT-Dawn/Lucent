//! `lucent history [--json]`: list History, newest first.

use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::daemon::with_daemon;
use lucent::engine::Urgency;
use lucent::text::ago::ago_phrase;
use lucent::text::markup::plain_text;

use crate::dbus::control::HistoryItem;

pub fn run(json: bool) -> ExitCode {
    with_daemon(|lucent| async move {
        let items = lucent.history().await?;
        if json {
            match serde_json::to_string_pretty(&items) {
                Ok(text) => println!("{text}"),
                Err(e) => {
                    eprintln!("lucent: can't write History as JSON: {e}");
                    return Ok(ExitCode::FAILURE);
                }
            }
        } else {
            print!("{}", readable(&items, unix_now()));
        }
        Ok(ExitCode::SUCCESS)
    })
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// History as a terminal list: one line per entry, with its body (as
/// plain text) on the line below.
fn readable(items: &[HistoryItem], now: u64) -> String {
    if items.is_empty() {
        return "History is empty.\n".into();
    }
    const AGE_WIDTH: usize = "just now".len();
    let id_width = items
        .iter()
        .map(|i| i.id.to_string().len())
        .max()
        .unwrap_or(1);
    let indent = " ".repeat(id_width + 2 + AGE_WIDTH + 2);

    let mut out = String::new();
    for item in items {
        let mut title = match item.app.as_str() {
            "" => item.summary.clone(),
            app => format!("{app}: {}", item.summary),
        };
        if item.urgency == Urgency::Critical {
            title.push_str("  [critical]");
        }
        let age = ago_phrase(Duration::from_secs(now.saturating_sub(item.time)));
        out += &format!("{:>id_width$}  {age:<AGE_WIDTH$}  {title}\n", item.id);
        let body = one_line(&plain_text(&item.body), BODY_WIDTH);
        if !body.is_empty() {
            out += &format!("{indent}{body}\n");
        }
    }
    out
}

/// Bodies longer than this many characters are cut short.
const BODY_WIDTH: usize = 80;

/// `text` on one line, whitespace collapsed, cut to at most `width`
/// characters.
fn one_line(text: &str, width: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= width {
        return line;
    }
    let mut cut: String = line.chars().take(width - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u32, app: &str, summary: &str, body: &str, time: u64) -> HistoryItem {
        HistoryItem {
            id,
            app: app.into(),
            summary: summary.into(),
            body: body.into(),
            time,
            ..HistoryItem::default()
        }
    }

    #[test]
    fn an_empty_history_says_so() {
        assert_eq!(readable(&[], 0), "History is empty.\n");
    }

    #[test]
    fn entries_show_id_age_app_summary_and_body() {
        let now = 100_000;
        let items = [
            item(
                12,
                "Firefox",
                "Download complete",
                "report.pdf is ready",
                now - 20,
            ),
            item(3, "Signal", "Alice", "", now - 5 * 60),
            item(1, "", "Backup finished", "", now - 3 * 3600),
        ];

        assert_eq!(
            readable(&items, now),
            "12  20s ago   Firefox: Download complete\n\
             \x20             report.pdf is ready\n\
             \x203  5m ago    Signal: Alice\n\
             \x201  3h ago    Backup finished\n"
        );
    }

    #[test]
    fn critical_entries_are_marked() {
        let critical = HistoryItem {
            urgency: Urgency::Critical,
            ..item(4, "Power", "Battery at 5%", "", 0)
        };

        assert_eq!(
            readable(&[critical], 2 * 86_400),
            "4  2d ago    Power: Battery at 5%  [critical]\n"
        );
    }

    #[test]
    fn bodies_are_shown_as_one_line_of_plain_text() {
        let body = "<b>Tom</b> &amp; Jerry\n  said <i>hi</i> &lt;3";

        assert_eq!(
            readable(&[item(1, "chat", "New message", body, 0)], 0),
            "1  just now  chat: New message\n\
             \x20            Tom & Jerry said hi <3\n"
        );
    }

    #[test]
    fn long_bodies_are_cut_short() {
        let body = "word ".repeat(40);

        let text = readable(&[item(1, "app", "s", &body, 0)], 0);

        let shown = text.lines().nth(1).unwrap().trim();
        assert_eq!(shown.chars().count(), 80);
        assert!(shown.ends_with('…'), "{shown}");
    }
}
