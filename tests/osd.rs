//! The OSD: tagged Notifications with a level, shown as one compact Popup
//! apart from the Stack, and `lucent osd`, which sends them.

use std::time::Duration;

use lucent::config::Config;
use lucent::core::{CloseReason, Core, Effect, Event, Notification, Popup};
use lucent::draw;
use lucent::osd::{self, Change, Level, Target};

fn level(tag: &str, value: i64) -> Notification {
    Notification {
        app_name: "Lucent".into(),
        summary: "Volume".into(),
        value: Some(value),
        tag: Some(tag.into()),
        ..Default::default()
    }
}

fn plain(summary: &str) -> Notification {
    Notification {
        summary: summary.into(),
        ..Default::default()
    }
}

fn accepted(effects: &[Effect]) -> u32 {
    match effects.first() {
        Some(Effect::Accepted(id)) => *id,
        other => panic!("expected an id first, got {other:?}"),
    }
}

fn stack(effects: &[Effect]) -> &[Popup] {
    effects
        .iter()
        .find_map(|e| match e {
            Effect::Stack(stack) => Some(stack.as_slice()),
            _ => None,
        })
        .expect("a Stack effect")
}

#[test]
fn a_tagged_level_shows_as_the_osd_after_the_stack() {
    let mut core = Core::new();
    core.handle(Event::Notify(plain("Hello")));

    let effects = core.handle(Event::Notify(level("volume", 40)));
    let id = accepted(&effects);
    let shown = stack(&effects);

    assert_eq!(shown.len(), 2);
    assert!(!shown[0].osd);
    assert_eq!((shown[1].id, shown[1].osd), (id, true));
    assert_eq!(shown[1].progress, Some(40));
    assert!(effects.contains(&Effect::StartTimer {
        id,
        after: Duration::from_millis(1500),
    }));
}

#[test]
fn the_same_tag_updates_the_osd_in_place() {
    let mut core = Core::new();
    let id = accepted(&core.handle(Event::Notify(level("volume", 40))));

    let effects = core.handle(Event::Notify(level("volume", 45)));

    assert_eq!(accepted(&effects), id);
    assert_eq!(stack(&effects)[0].progress, Some(45));
    assert!(!effects.iter().any(|e| matches!(e, Effect::Closed { .. })));
}

#[test]
fn a_different_osd_takes_the_place_of_the_one_up() {
    let mut core = Core::new();
    let volume = accepted(&core.handle(Event::Notify(level("volume", 40))));

    let effects = core.handle(Event::Notify(level("brightness", 70)));
    let brightness = accepted(&effects);

    assert_ne!(brightness, volume);
    assert_eq!(stack(&effects).len(), 1);
    assert_eq!(stack(&effects)[0].id, brightness);
    assert!(effects.contains(&Effect::Closed {
        id: volume,
        reason: CloseReason::Expired,
    }));
}

#[test]
fn osds_never_fill_the_stack_or_wait_in_the_queue() {
    let (config, _) = Config::parse("general.max_visible = 1");
    let mut core = Core::with_config(config);
    core.handle(Event::Notify(plain("Hello")));

    let effects = core.handle(Event::Notify(level("volume", 40)));

    assert_eq!(stack(&effects).len(), 2);
    assert_eq!(stack(&effects)[0].more, 0);
}

#[test]
fn a_tag_without_a_level_replaces_an_ordinary_popup_in_place() {
    let mut core = Core::new();
    let tagged = |summary: &str| Notification {
        tag: Some("sync".into()),
        ..plain(summary)
    };
    let id = accepted(&core.handle(Event::Notify(tagged("first"))));

    let effects = core.handle(Event::Notify(tagged("second")));

    assert_eq!(accepted(&effects), id);
    let shown = stack(&effects);
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].summary, "second");
    assert!(!shown[0].osd);
}

#[test]
fn with_the_osd_off_a_tagged_level_is_an_ordinary_popup() {
    let (config, _) = Config::parse("osd.enabled = false");
    let mut core = Core::with_config(config);

    let effects = core.handle(Event::Notify(level("volume", 40)));

    assert!(!stack(&effects)[0].osd);
    assert!(effects.contains(&Effect::StartTimer {
        id: accepted(&effects),
        after: Duration::from_secs(5),
    }));
}

#[test]
fn the_osd_expires_and_lets_lucent_go_idle() {
    let mut core = Core::new();
    let id = accepted(&core.handle(Event::Notify(level("volume", 40))));

    let effects = core.handle(Event::TimerFired(id));

    assert_eq!(stack(&effects), []);
    assert!(effects.contains(&Effect::Closed {
        id,
        reason: CloseReason::Expired,
    }));
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::StartIdleTimer { .. }))
    );
}

#[test]
fn the_osd_can_be_clicked_away_and_closed() {
    let mut core = Core::new();
    let id = accepted(&core.handle(Event::Notify(level("volume", 40))));
    let effects = core.handle(Event::Close(id));
    assert_eq!(stack(&effects), []);
    assert!(effects.contains(&Effect::Closed {
        id,
        reason: CloseReason::Closed,
    }));
}

#[test]
fn replacing_an_osd_by_id_with_a_plain_popup_moves_it_to_the_stack() {
    let mut core = Core::new();
    let id = accepted(&core.handle(Event::Notify(level("volume", 40))));

    let effects = core.handle(Event::Notify(Notification {
        replaces_id: id,
        ..plain("plain now")
    }));

    assert_eq!(accepted(&effects), id);
    let shown = stack(&effects);
    assert_eq!(shown.len(), 1);
    assert!(!shown[0].osd);
}

#[test]
fn the_osd_is_one_compact_row() {
    let mut core = Core::new();
    let effects = core.handle(Event::Notify(level("volume", 40)));
    let osd = &stack(&effects)[0];
    let popup = Popup {
        osd: false,
        ..osd.clone()
    };

    let compact = draw::measure(osd, None, 300);

    assert!(compact < draw::measure(&popup, None, 300));
    assert!(compact < 60, "{compact} px tall");
}

#[test]
fn osd_arguments_parse() {
    let args = |s: &str| s.split_whitespace().map(String::from).collect::<Vec<_>>();
    assert_eq!(
        osd::parse(&args("volume up")),
        Ok((Target::Volume, Change::Up(5)))
    );
    assert_eq!(
        osd::parse(&args("volume down 10%")),
        Ok((Target::Volume, Change::Down(10)))
    );
    assert_eq!(
        osd::parse(&args("mic mute")),
        Ok((Target::Mic, Change::Mute))
    );
    assert_eq!(
        osd::parse(&args("brightness up 2")),
        Ok((Target::Brightness, Change::Up(2)))
    );
    assert_eq!(
        osd::parse(&args("brightness show")),
        Ok((Target::Brightness, Change::Show))
    );
    for bad in [
        "",
        "volume",
        "brightness mute",
        "volume up 0",
        "volume up 5 6",
        "bass up",
    ] {
        assert!(osd::parse(&args(bad)).is_err(), "{bad:?} parsed");
    }
}

#[test]
fn wpctl_and_brightnessctl_output_is_read() {
    assert_eq!(
        osd::parse_wpctl("Volume: 0.45\n"),
        Some(Level {
            percent: 45,
            muted: false
        })
    );
    assert_eq!(
        osd::parse_wpctl("Volume: 1.00 [MUTED]\n"),
        Some(Level {
            percent: 100,
            muted: true
        })
    );
    assert_eq!(osd::parse_wpctl("nope"), None);
    assert_eq!(
        osd::parse_brightnessctl("intel_backlight,backlight,1234,45%,2700\n"),
        Some(Level {
            percent: 45,
            muted: false
        })
    );
    assert_eq!(osd::parse_brightnessctl("garbage"), None);
}

#[test]
fn volume_icons_follow_the_level() {
    let at = |percent, muted| osd::icons(Target::Volume, Level { percent, muted })[0];
    assert_eq!(at(50, true), "audio-volume-muted");
    assert_eq!(at(0, false), "audio-volume-muted");
    assert_eq!(at(20, false), "audio-volume-low");
    assert_eq!(at(50, false), "audio-volume-medium");
    assert_eq!(at(90, false), "audio-volume-high");
}

/// Runs `lucent osd <args>` against a fake `wpctl` whose `get-volume` prints
/// `volume`, `wpctl_stderr` going to stderr; returns the calls it got and
/// Lucent's stderr. The bus is unreachable, so nothing is shown.
fn osd_with_fake_wpctl(args: &[&str], volume: &str, wpctl_stderr: &str) -> (String, String) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("calls");
    let fake = dir.path().join("wpctl");
    std::fs::write(
        &fake,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\n[ \"$1\" = get-volume ] && printf '%s' '{volume}'\nprintf '%s' '{wpctl_stderr}' >&2\nexit 0\n",
            log.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_lucent"))
        .arg("osd")
        .args(args)
        .env("PATH", dir.path())
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent/lucent-test",
        )
        .output()
        .unwrap();
    (
        std::fs::read_to_string(&log).unwrap_or_default(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn volume_up_never_lowers_a_volume_already_above_100_percent() {
    let (calls, _) = osd_with_fake_wpctl(&["volume", "up"], "Volume: 1.50\n", "");

    // `-l 1.0` would have clamped 150% down to 100%.
    assert!(
        !calls.contains("-l 1.0 "),
        "volume up at 150% was limited to 100%: {calls}"
    );
}

#[test]
fn volume_up_still_stops_at_100_percent() {
    let (calls, _) = osd_with_fake_wpctl(&["volume", "up"], "Volume: 0.98\n", "");

    assert!(
        calls.contains("set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ 5%+"),
        "{calls}"
    );
}

#[test]
fn a_missing_audio_device_is_reported_with_wpctl_s_reason() {
    let (_, stderr) = osd_with_fake_wpctl(&["mic", "show"], "", "Node 99 not found");

    assert!(stderr.contains("wpctl: Node 99 not found"), "{stderr}");
}

#[test]
fn turning_a_popup_into_the_osd_lets_the_queue_move_up() {
    let (config, _) = Config::parse("general.max_visible = 1");
    let mut core = Core::with_config(config);
    core.handle(Event::Notify(Notification {
        tag: Some("volume".into()),
        ..plain("tagged")
    }));
    core.handle(Event::Notify(plain("waiting")));

    let effects = core.handle(Event::Notify(level("volume", 50)));

    let shown = stack(&effects);
    assert!(
        shown.iter().any(|p| p.summary == "waiting" && !p.osd),
        "the queued Popup stayed hidden: {shown:?}"
    );
}

#[test]
fn a_tag_names_the_newest_notification_that_took_it() {
    // A sender-chosen replaces_id can hand a live Tag to a second id; the
    // next tagged Notification must replace that newest one, every time.
    for _ in 0..32 {
        let mut core = Core::new();
        core.handle(Event::Notify(Notification {
            tag: Some("t".into()),
            ..plain("first")
        }));
        let newest = accepted(&core.handle(Event::Notify(Notification {
            replaces_id: 99,
            tag: Some("t".into()),
            ..plain("second")
        })));

        let effects = core.handle(Event::Notify(Notification {
            tag: Some("t".into()),
            ..plain("third")
        }));

        assert_eq!(accepted(&effects), newest);
    }
}
