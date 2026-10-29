//! Seam 1, Theme half: config text and Urgency in, each Popup's resolved
//! Theme out.

use lucent::config::{self, Config, Problem};
use lucent::core::{Core, Effect, Event, Notification, Urgency};
use lucent::theme::{Rgba, Theme};

fn notification(urgency: Urgency) -> Notification {
    Notification {
        app_name: "notify-send".into(),
        summary: "Hello".into(),
        body: "World".into(),
        urgency,
        ..Default::default()
    }
}

/// The Theme the core resolves for a Notification of `urgency` under `config`.
fn theme_for(config: &str, urgency: Urgency) -> Theme {
    let (config, _) = Config::parse(config);
    let mut core = Core::with_config(config);
    core.handle(Event::Notify(notification(urgency)))
        .into_iter()
        .find_map(|e| match e {
            Effect::Stack(popups) => popups.into_iter().next().map(|p| p.theme),
            _ => None,
        })
        .expect("a Notify should show a Popup")
}

fn hex(rgb: u32, alpha: u8) -> Rgba {
    Rgba::new((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, alpha)
}

#[test]
fn with_no_config_popups_wear_midnight_bloom() {
    let theme = theme_for("", Urgency::Normal);

    // The values from the spec (#18); 82% alpha is 209 of 255.
    assert_eq!(theme.background, hex(0x141B1F, 209));
    assert_eq!(theme.border, Rgba::new(252, 253, 253, 33));
    assert_eq!(theme.border_width, 1);
    assert_eq!(theme.radius, 20);
    assert_eq!(theme.summary, hex(0xC0CAF5, 255));
    assert_eq!(theme.body, hex(0xA9B1D6, 255));
    assert_eq!(theme.app_name, hex(0x92A8A9, 255));
    assert_eq!(theme.accent, hex(0x8DE3DC, 255));
    assert_eq!(theme.font, "Inter, sans-serif");
    assert!(
        theme.border_top.a > theme.border.a,
        "the top edge should be brighter: {theme:?}"
    );
}

#[test]
fn a_preset_is_chosen_by_name() {
    let bloom = theme_for("", Urgency::Normal);
    let light = theme_for("theme.preset = \"light\"", Urgency::Normal);
    let opaque = theme_for("theme.preset = \"opaque\"", Urgency::Normal);

    assert_eq!(
        theme_for("theme.preset = \"midnight-bloom\"", Urgency::Normal),
        bloom
    );
    assert_eq!(Theme::preset("light"), Some(light.clone()));
    assert_eq!(Theme::preset("opaque"), Some(opaque.clone()));
    assert_ne!(light, bloom);
    assert_ne!(opaque, bloom);
    assert_ne!(light, opaque);
    assert_eq!(opaque.background.a, 255, "the opaque Preset needs no blur");
}

#[test]
fn an_unknown_preset_is_reported_and_falls_back_to_midnight_bloom() {
    let text = "[theme]\npreset = \"tokyo\"";

    let (_, problems) = Config::parse(text);
    assert_eq!(problems, [Problem::UnknownPreset("tokyo".into())]);
    assert_eq!(
        theme_for(text, Urgency::Normal),
        theme_for("", Urgency::Normal)
    );
}

#[test]
fn field_overrides_apply_on_top_of_the_chosen_preset() {
    let text = r##"
        [theme]
        preset = "opaque"
        accent = "#FF8800"
        border = "rgba(10, 20, 30, 0.5)"
        summary = "#11223380"
        radius = 6
        font = "Iosevka"
    "##;

    let (_, problems) = Config::parse(text);
    assert_eq!(problems, []);
    let mut expected = Theme::preset("opaque").unwrap();
    expected.accent = Rgba::new(0xFF, 0x88, 0x00, 255);
    expected.border = Rgba::new(10, 20, 30, 128);
    expected.summary = Rgba::new(0x11, 0x22, 0x33, 0x80);
    expected.radius = 6;
    expected.font = "Iosevka".into();
    assert_eq!(theme_for(text, Urgency::Normal), expected);
}

#[test]
fn a_bad_colour_spoils_the_file_and_an_unknown_theme_key_is_skipped() {
    let (config, problems) = Config::parse("[theme]\naccent = \"mint\"");
    assert_eq!(config, Config::default());
    assert!(
        matches!(problems.as_slice(), [Problem::Invalid(_)]),
        "{problems:?}"
    );

    let (_, problems) = Config::parse("[theme]\nshadow = \"#000000\"");
    assert_eq!(problems, [Problem::UnknownKey("theme.shadow".into())]);
}

#[test]
fn critical_popups_wear_rose_and_low_ones_are_dimmed() {
    let low = theme_for("", Urgency::Low);
    let normal = theme_for("", Urgency::Normal);
    let critical = theme_for("", Urgency::Critical);

    assert_eq!(normal, Theme::preset("midnight-bloom").unwrap());
    let rose = hex(0xF7768E, 255);
    assert_eq!((critical.border, critical.accent), (rose, rose));
    assert_eq!(critical.background, normal.background);
    for (dim, full) in [
        (low.summary, normal.summary),
        (low.body, normal.body),
        (low.app_name, normal.app_name),
    ] {
        assert!(dim.a < full.a, "low-urgency text should be dimmed: {low:?}");
    }
    assert_ne!(low, critical);
}

#[test]
fn the_urgency_styles_follow_the_theme_overrides() {
    let text = "[theme]\ncritical = \"#FF0000\"\nlow_opacity = 50";

    let critical = theme_for(text, Urgency::Critical);
    assert_eq!(critical.accent, hex(0xFF0000, 255));
    assert_eq!(theme_for(text, Urgency::Low).summary, hex(0xC0CAF5, 128));
}

/// Every Theme field is listed, commented out so a copied `[theme]` still
/// follows whichever Preset it names.
#[test]
fn the_reference_config_documents_every_theme_field_and_preset() {
    let theme = &config::REFERENCE[config::REFERENCE
        .find("\n[theme]")
        .expect("a [theme] section")..];
    for field in Theme::FIELDS {
        assert!(
            theme
                .lines()
                .any(|l| l.starts_with(&format!("# {field} = "))),
            "`{field}` is not documented under [theme]"
        );
    }
    for (preset, _) in Theme::PRESETS {
        assert!(theme.contains(&format!("\"{preset}\"")), "{preset}");
    }
}

#[test]
fn the_documented_theme_values_are_midnight_bloom() {
    let uncommented: String = config::REFERENCE
        .lines()
        .map(|l| match l.strip_prefix("# ") {
            Some(rest)
                if Theme::FIELDS
                    .iter()
                    .any(|f| rest.starts_with(&format!("{f} = "))) =>
            {
                rest
            }
            _ => l,
        })
        .map(|l| format!("{l}\n"))
        .collect();

    let (config, problems) = Config::parse(&uncommented);
    assert_eq!(problems, []);
    assert_eq!(config.theme(), Theme::preset("midnight-bloom").unwrap());
}

#[test]
fn icons_are_48_px_from_the_system_icon_theme_unless_overridden() {
    let default = theme_for("", Urgency::Normal);
    assert_eq!((default.icon_size, default.icon_theme.as_str()), (48, ""));

    let set = theme_for(
        "[theme]\nicon_size = 64\nicon_theme = \"Papirus-Dark\"",
        Urgency::Normal,
    );
    assert_eq!(
        (set.icon_size, set.icon_theme.as_str()),
        (64, "Papirus-Dark")
    );
}
