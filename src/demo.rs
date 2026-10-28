//! The script of `lucent demo`: a short, playful showcase of every kind of
//! Popup. Pure data, so tests can play it through the engine; the CLI
//! sends it over D-Bus.

use std::sync::Arc;
use std::time::Duration;

use crate::engine::{Action, Expiry, Hints, Image, ImageData, Notification, Urgency};

const PLAIN: u32 = 1;
const LOW: u32 = 2;
const ACTIONS: u32 = 3;
const ART: u32 = 4;
const PROGRESS: u32 = 5;
const CRITICAL: u32 = 6;

/// How long the Popups that make up the finale stay, so there is time to
/// look at them (and hover, click, or take a screenshot).
const LINGER: Duration = Duration::from_secs(14);

/// One beat of the demo: wait, then send a Notification. Ids are local to
/// the script; a step that reuses an earlier step's id replaces it.
#[derive(Debug, Clone)]
pub struct Step {
    pub wait: Duration,
    pub notification: Notification,
}

/// The whole demo, in order.
pub fn script() -> Vec<Step> {
    let mut steps = vec![
        step(
            0,
            popup(LOW, "Lucent", "").urgency(Urgency::Low).text(
                "Psst, a low one",
                "Low Urgency Popups keep their voice down and leave early. \
                 I'll see myself out.",
            ),
        ),
        step(
            600,
            popup(PLAIN, "Lucent", "")
                .text(
                    "Hello from Lucent",
                    "Notifications that don't nag, they glow. \
                 Hover a Popup to pause its countdown.",
                )
                .linger(),
        ),
        step(
            600,
            popup(ACTIONS, "Coffee", "")
                .text(
                    "Coffee break?",
                    "Your mug has been empty for <b>47 minutes</b>. \
                 That's a cry for help.",
                )
                .action("default", "Open")
                .action("brew", "Brew one")
                .action("later", "Later")
                .linger(),
        ),
        step(
            600,
            popup(ART, "Music", "audio-x-generic")
                .text(
                    "Glow in the Dark",
                    "The Lucents · <i>Popups at Midnight</i>",
                )
                .art(album_art())
                .linger(),
        ),
    ];
    // A download that fills its progress ring in place, one Popup throughout.
    let progress = [0, 8, 21, 34, 47, 58, 66, 79, 88, 95];
    for (i, percent) in progress.into_iter().enumerate() {
        steps.push(step(
            if i == 0 { 600 } else { 450 },
            popup(PROGRESS, "Downloads", "folder-download")
                .text(
                    "Downloading more RAM",
                    "16 GB of pure speed. Totally legit.",
                )
                .progress(percent)
                .linger(),
        ));
        if i == 2 {
            // The low one has gone by now, so there's room at the top.
            steps.push(step(
                0,
                popup(CRITICAL, "Battery", "")
                    .urgency(Urgency::Critical)
                    .text(
                        "Battery at 2%",
                        "Critical Popups stay until you dismiss them, \
                         even in Do Not Disturb. Relax, it's a demo.",
                    ),
            ));
        }
    }
    steps.push(step(
        450,
        popup(PROGRESS, "Downloads", "folder-download")
            .text(
                "More RAM downloaded",
                "Your computer feels exactly the same.",
            )
            .progress(100)
            .linger(),
    ));
    steps
}

fn step(wait_ms: u64, popup: DemoPopup) -> Step {
    Step {
        wait: Duration::from_millis(wait_ms),
        notification: popup.0,
    }
}

fn popup(id: u32, app: &str, icon: &str) -> DemoPopup {
    DemoPopup(Notification {
        id,
        app_name: app.into(),
        app_icon: icon.into(),
        summary: String::new(),
        body: String::new(),
        actions: Vec::new(),
        hints: Hints::default(),
        expiry: Expiry::Default,
    })
}

struct DemoPopup(Notification);

impl DemoPopup {
    fn text(mut self, summary: &str, body: &str) -> Self {
        self.0.summary = summary.into();
        self.0.body = body.into();
        self
    }

    fn urgency(mut self, urgency: Urgency) -> Self {
        self.0.hints.urgency = urgency;
        self
    }

    fn action(mut self, key: &str, label: &str) -> Self {
        self.0.actions.push(Action {
            key: key.into(),
            label: label.into(),
        });
        self
    }

    fn art(mut self, art: ImageData) -> Self {
        self.0.hints.image = Some(Image::Data(art));
        self
    }

    fn progress(mut self, percent: u8) -> Self {
        self.0.hints.progress = Some(percent);
        self
    }

    fn linger(mut self) -> Self {
        self.0.expiry = Expiry::After(LINGER);
        self
    }
}

/// Side of the generated album cover, in pixels.
const ART_SIDE: usize = 128;

/// A small synthwave sunset for the "now playing" Popup, drawn here so the
/// demo needs no files: a violet-to-coral sky, a glowing sun and a dark sea
/// that reflects it. RGBA, 8 bits per sample, rows packed.
pub fn album_art() -> ImageData {
    let side = ART_SIDE as f32;
    let mut data = Vec::with_capacity(ART_SIDE * ART_SIDE * 4);
    let mix = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
        let t = t.clamp(0.0, 1.0);
        [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
    };
    let horizon = 0.68;
    let (sun_x, sun_y, sun_r) = (0.5, 0.6, 0.26);
    for y in 0..ART_SIDE {
        for x in 0..ART_SIDE {
            let (u, v) = ((x as f32 + 0.5) / side, (y as f32 + 0.5) / side);
            let rgb = if v < horizon {
                let sky = mix([42.0, 22.0, 84.0], [255.0, 110.0, 110.0], v / horizon);
                let d = ((u - sun_x).powi(2) + (v - sun_y).powi(2)).sqrt();
                // Sun with retro bands cut out of its lower half.
                let band = v > sun_y - 0.02 && ((v - sun_y) * 60.0).rem_euclid(5.0) < 1.6;
                if d < sun_r && !band {
                    mix(
                        [255.0, 214.0, 120.0],
                        [255.0, 120.0, 140.0],
                        (v - 0.34) / 0.3,
                    )
                } else {
                    let glow = (1.0 - (d - sun_r) / 0.25).clamp(0.0, 1.0).powi(2) * 0.45;
                    mix(sky, [255.0, 190.0, 140.0], glow)
                }
            } else {
                let depth = (v - horizon) / (1.0 - horizon);
                let sea = mix([40.0, 18.0, 70.0], [14.0, 8.0, 32.0], depth);
                // Broken reflection lines under the sun.
                let width = sun_r * (1.0 - depth * 0.6);
                let line = (u - sun_x).abs() < width && ((depth * 40.0) as i32) % 3 == 0;
                if line {
                    mix(sea, [255.0, 150.0, 140.0], 0.65 - depth * 0.4)
                } else {
                    sea
                }
            };
            data.extend(rgb.map(|c| c.round() as u8));
            data.push(255);
        }
    }
    ImageData {
        width: ART_SIDE as i32,
        height: ART_SIDE as i32,
        rowstride: (ART_SIDE * 4) as i32,
        has_alpha: true,
        bits_per_sample: 8,
        channels: 4,
        data: Arc::from(data),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::engine::testing::{Harness, overflow_count, removed, shown, updated};
    use crate::engine::{NotificationId, Urgency};

    use super::*;

    /// Play the script through the engine with default settings and
    /// return, after each step, the visible Notifications (oldest first).
    fn play() -> Vec<Vec<Notification>> {
        let mut h = Harness::new();
        let mut visible: BTreeMap<NotificationId, Notification> = BTreeMap::new();
        let mut order: Vec<NotificationId> = Vec::new();
        let mut frames = Vec::new();
        for step in script() {
            let mut effects = h.advance(step.wait);
            effects.extend(h.notify(step.notification.clone()));
            assert_eq!(overflow_count(&effects).unwrap_or(0), 0, "nothing waits");
            for (id, _) in removed(&effects) {
                visible.remove(&id);
                order.retain(|o| *o != id);
            }
            for id in shown(&effects) {
                order.push(id);
            }
            if order.contains(&step.notification.id) {
                visible.insert(step.notification.id, step.notification.clone());
            }
            frames.push(order.iter().map(|id| visible[id].clone()).collect());
        }
        frames
    }

    fn every_notification() -> Vec<Notification> {
        script().into_iter().map(|s| s.notification).collect()
    }

    #[test]
    fn the_demo_shows_every_popup_kind() {
        let all = every_notification();
        let plain = |n: &&Notification| {
            n.hints.urgency == Urgency::Normal
                && n.actions.is_empty()
                && n.hints.image.is_none()
                && n.hints.progress.is_none()
        };
        assert!(all.iter().any(|n| plain(&n)), "plain");
        assert!(all.iter().any(|n| !n.actions.is_empty()), "Actions");
        assert!(all.iter().any(|n| n.hints.image.is_some()), "album art");
        assert!(all.iter().any(|n| n.hints.progress.is_some()), "progress");
        assert!(all.iter().any(|n| n.hints.urgency == Urgency::Low), "low");
        assert!(
            all.iter().any(|n| n.hints.urgency == Urgency::Critical),
            "Critical"
        );
        let frames = play();
        let ever_shown: Vec<_> = frames.iter().flatten().map(|n| n.id).collect();
        for n in &all {
            assert!(ever_shown.contains(&n.id), "{} never shown", n.summary);
        }
    }

    #[test]
    fn the_finale_shows_every_lasting_kind_at_once_with_room_to_spare() {
        let finale = play().pop().unwrap();
        let summaries: Vec<_> = finale.iter().map(|n| n.summary.as_str()).collect();
        assert_eq!(
            summaries,
            [
                "Hello from Lucent",
                "Coffee break?",
                "Glow in the Dark",
                "More RAM downloaded",
                "Battery at 2%",
            ]
        );
    }

    #[test]
    fn the_download_fills_one_ring_to_100_in_place() {
        let mut h = Harness::new();
        let mut percents = Vec::new();
        let mut shows = 0;
        for step in script() {
            h.advance(step.wait);
            let effects = h.notify(step.notification.clone());
            if let Some(percent) = step.notification.hints.progress {
                percents.push(percent);
                shows += shown(&effects).len();
                if percents.len() > 1 {
                    assert_eq!(updated(&effects), [step.notification.id]);
                }
            }
        }
        assert_eq!(shows, 1);
        assert!(percents.is_sorted(), "{percents:?}");
        assert_eq!(percents.first(), Some(&0));
        assert_eq!(percents.last(), Some(&100));
    }

    #[test]
    fn the_demo_is_over_in_a_few_seconds() {
        let total: Duration = script().iter().map(|s| s.wait).sum();
        assert!(total <= Duration::from_secs(8), "{total:?}");
    }

    #[test]
    fn only_the_critical_popup_waits_for_you() {
        let mut h = Harness::new();
        for step in script() {
            h.advance(step.wait);
            h.notify(step.notification);
        }
        h.advance(Duration::from_secs(60));
        assert_eq!(h.engine.newest_popup(), Some(CRITICAL));
        assert_eq!(h.engine.status().history_count, 6);
    }
}
