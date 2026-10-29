//! Seam 1, Rules: config text and a Notification in, the Popup's resolved
//! Theme and timeout out.

use std::time::Duration;

use lucent::config::{Config, Problem, REFERENCE};
use lucent::core::{Core, Effect, Event, Notification, Popup, Timeout, Urgency};
use lucent::theme::Rgba;

fn sent(app_name: &str, urgency: Urgency) -> Notification {
    Notification {
        app_name: app_name.into(),
        summary: "s".into(),
        urgency,
        ..Default::default()
    }
}

/// The Popup a fresh core shows for `n` under `config`, and the timer it
/// starts (if any).
fn shown(config: &str, n: Notification) -> (Popup, Option<Duration>) {
    let (config, problems) = Config::parse(config);
    assert_eq!(problems, [], "the test config should be clean");
    let effects = Core::with_config(config).handle(Event::Notify(n));
    let popup = effects
        .iter()
        .find_map(|e| match e {
            Effect::Stack(popups) => popups.first().cloned(),
            _ => None,
        })
        .expect("a Notify should show a Popup");
    let timer = effects.iter().find_map(|e| match e {
        Effect::StartTimer { after, .. } => Some(*after),
        _ => None,
    });
    (popup, timer)
}

const RED: Rgba = Rgba::new(255, 0, 0, 255);
const BLUE: Rgba = Rgba::new(0, 0, 255, 255);

#[test]
fn a_rule_matches_by_app_name() {
    let text = r##"
        [[rule]]
        app_name = "Spotify"
        theme = { radius = 4, accent = "#FF0000" }
    "##;

    let (spotify, _) = shown(text, sent("Spotify", Urgency::Normal));
    assert_eq!(spotify.theme.radius, 4);
    assert_eq!(spotify.theme.accent, RED);

    let (other, _) = shown(text, sent("Firefox", Urgency::Normal));
    assert_eq!(
        other.theme,
        shown("", sent("Firefox", Urgency::Normal)).0.theme
    );
}

#[test]
fn a_rule_matches_by_urgency() {
    let text = r##"
        [[rule]]
        urgency = "low"
        theme = { padding = 8 }
    "##;

    assert_eq!(shown(text, sent("a", Urgency::Low)).0.theme.padding, 8);
    assert_eq!(shown(text, sent("a", Urgency::Normal)).0.theme.padding, 14);
    assert_eq!(
        shown(text, sent("a", Urgency::Critical)).0.theme.padding,
        14
    );
}

#[test]
fn a_rule_with_both_matchers_needs_both_to_match() {
    let text = r##"
        [[rule]]
        app_name = "Spotify"
        urgency = "critical"
        theme = { radius = 0 }
    "##;

    assert_eq!(
        shown(text, sent("Spotify", Urgency::Critical))
            .0
            .theme
            .radius,
        0
    );
    assert_eq!(
        shown(text, sent("Spotify", Urgency::Normal)).0.theme.radius,
        18
    );
    assert_eq!(
        shown(text, sent("Firefox", Urgency::Critical))
            .0
            .theme
            .radius,
        18
    );
}

#[test]
fn later_matching_rules_override_earlier_ones_and_others_do_not_apply() {
    let text = r##"
        [theme]
        radius = 12

        [[rule]]
        app_name = "Spotify"
        theme = { accent = "#FF0000", padding = 4 }

        [[rule]]
        app_name = "Firefox"
        theme = { padding = 30 }

        [[rule]]
        urgency = "normal"
        theme = { accent = "#0000FF" }
    "##;

    let theme = shown(text, sent("Spotify", Urgency::Normal)).0.theme;
    assert_eq!(theme.accent, BLUE, "the later Rule wins");
    assert_eq!(theme.padding, 4, "the Firefox Rule does not apply");
    assert_eq!(theme.radius, 12, "Rules sit on top of [theme]");
}

#[test]
fn a_rule_can_restyle_critical_notifications() {
    let text = r##"
        [[rule]]
        urgency = "critical"
        theme = { critical = "#0000FF" }
    "##;

    let theme = shown(text, sent("a", Urgency::Critical)).0.theme;
    assert_eq!(theme.accent, BLUE);
    assert_eq!(theme.border, BLUE.faded(55));
}

#[test]
fn a_rule_can_override_the_timeout() {
    let text = r##"
        [[rule]]
        app_name = "Spotify"
        timeout = 1500
    "##;
    let asked = |timeout| Notification {
        timeout,
        ..sent("Spotify", Urgency::Normal)
    };

    for timeout in [
        Timeout::Default,
        Timeout::Never,
        Timeout::After(Duration::from_secs(9)),
    ] {
        assert_eq!(
            shown(text, asked(timeout)).1,
            Some(Duration::from_millis(1500)),
            "the Rule beats the app's request of {timeout:?}"
        );
    }
    assert_eq!(
        shown(text, sent("Firefox", Urgency::Normal)).1,
        Some(Duration::from_secs(5))
    );
}

#[test]
fn a_rule_cannot_make_a_critical_notification_expire() {
    let text = r##"
        [[rule]]
        timeout = 1500

        [[rule]]
        urgency = "critical"
        timeout = 3000
    "##;

    let (popup, timer) = shown(text, sent("a", Urgency::Critical));
    assert_eq!(timer, None);
    assert_eq!(popup.timeout, None);
}

/// The reference config's Rule example, indented under `#   `, works once
/// uncommented.
#[test]
fn the_reference_config_documents_rules_with_a_working_example() {
    let rules = &REFERENCE[REFERENCE.find("\n# [[rule]]").expect("a [[rule]] section")..];
    let example: String = rules
        .lines()
        .filter_map(|l| l.strip_prefix("#   "))
        .map(|l| format!("{l}\n"))
        .collect();

    let (config, problems) = Config::parse(&example);
    assert_eq!(problems, []);
    assert!(config.rules.len() >= 2, "{example}");
    let (spotify, _) = shown(&example, sent("Spotify", Urgency::Normal));
    assert_ne!(
        spotify.theme,
        shown("", sent("Spotify", Urgency::Normal)).0.theme
    );
}

#[test]
fn an_unknown_preset_in_a_rule_is_reported_and_falls_back_to_midnight_bloom() {
    let text = r##"
        theme.preset = "light"

        [[rule]]
        app_name = "Spotify"
        theme = { preset = "tokyo", radius = 4 }
    "##;

    let (config, problems) = Config::parse(text);
    let effects = Core::with_config(config).handle(Event::Notify(sent("Spotify", Urgency::Normal)));
    let theme = effects.into_iter().find_map(|e| match e {
        Effect::Stack(popups) => popups.into_iter().next().map(|p| p.theme),
        _ => None,
    });

    assert_eq!(
        problems,
        [Problem::UnknownRulePreset {
            rule: 1,
            name: "tokyo".into()
        }]
    );
    let bloom_with_radius_4 = shown("theme.radius = 4", sent("Spotify", Urgency::Normal)).0;
    assert_eq!(theme, Some(bloom_with_radius_4.theme));
}
