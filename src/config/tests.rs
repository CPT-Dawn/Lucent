//! Seam 2: config text in, config plus warnings (or errors) out.

use std::time::Duration;

use super::*;

fn ok(text: &str) -> Parsed {
    parse(text).unwrap_or_else(|rejected| panic!("rejected: {rejected:#?}"))
}

fn config(text: &str) -> Config {
    let parsed = ok(text);
    assert!(parsed.warnings.is_empty(), "{:#?}", parsed.warnings);
    parsed.config
}

fn errors(text: &str) -> Vec<Diagnostic> {
    match parse(text) {
        Ok(parsed) => panic!("accepted: {parsed:#?}"),
        Err(rejected) => rejected.errors,
    }
}

/// The only error for `text`.
fn error(text: &str) -> Diagnostic {
    let mut errors = errors(text);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    errors.remove(0)
}

fn warning_lines(text: &str) -> Vec<(usize, String)> {
    ok(text)
        .warnings
        .into_iter()
        .map(|w| (w.line, w.message))
        .collect()
}

#[test]
fn an_empty_file_gives_the_full_defaults() {
    assert_eq!(config(""), Config::default());
    assert_eq!(config("\n# just a comment\n\n"), Config::default());
}

#[test]
fn the_defaults_are_the_spec_defaults() {
    let c = Config::default();
    assert_eq!(c.theme, Theme::Auto);
    assert_eq!(
        c.popup,
        PopupConfig {
            position: Position {
                vertical: Vertical::Top,
                horizontal: Horizontal::Right
            },
            width: 380,
            max_visible: 5,
            gap: 10,
            margin: 16,
            output: None,
        }
    );
    assert_eq!(c.timeouts.low, Some(Duration::from_secs(3)));
    assert_eq!(c.timeouts.normal, Some(Duration::from_secs(5)));
    assert_eq!(c.timeouts.critical, None);
    assert_eq!(
        c.look,
        LookConfig {
            accent: None,
            opacity: 1.0,
            radius: 14,
            font: None,
            icon_size: 40,
            countdown: true,
        }
    );
    assert_eq!(c.history.limit, 100);
    assert!(c.dnd.allow_critical);
}

#[test]
fn a_partial_file_merges_with_the_defaults() {
    let c = config(
        r#"
theme = "graphite"

[popup]
width = 420

[timeout]
critical = 30
"#,
    );

    let defaults = Config::default();
    let expected = Config {
        theme: Theme::Graphite,
        popup: PopupConfig {
            width: 420,
            ..defaults.popup.clone()
        },
        timeouts: Timeouts {
            critical: Some(Duration::from_secs(30)),
            ..defaults.timeouts
        },
        ..defaults
    };
    assert_eq!(c, expected);
}

#[test]
fn every_key_can_be_set() {
    let c = config(
        r##"
theme = "dawn"

[popup]
position = "bottom-center"
width = 400
max_visible = 3
gap = 6
margin = 24
output = "DP-1"

[timeout]
low = 2
normal = 7.5
critical = 0

[look]
accent = "#ff8a3d"
opacity = 0.5
radius = 0
font = "Inter"
icon_size = 32
countdown = false

[history]
limit = 20

[dnd]
allow_critical = false
"##,
    );

    assert_eq!(c.theme, Theme::Dawn);
    assert_eq!(
        c.popup,
        PopupConfig {
            position: Position {
                vertical: Vertical::Bottom,
                horizontal: Horizontal::Center
            },
            width: 400,
            max_visible: 3,
            gap: 6,
            margin: 24,
            output: Some("DP-1".into()),
        }
    );
    assert_eq!(c.timeouts.low, Some(Duration::from_secs(2)));
    assert_eq!(c.timeouts.normal, Some(Duration::from_millis(7_500)));
    assert_eq!(c.timeouts.critical, None);
    assert_eq!(
        c.look,
        LookConfig {
            accent: Some("#ff8a3d".into()),
            opacity: 0.5,
            radius: 0,
            font: Some("Inter".into()),
            icon_size: 32,
            countdown: false,
        }
    );
    assert_eq!(c.history.limit, 20);
    assert!(!c.dnd.allow_critical);
}

#[test]
fn a_zero_timeout_means_never() {
    let c = config("[timeout]\nlow = 0\nnormal = 0\n");
    assert_eq!(c.timeouts.low, None);
    assert_eq!(c.timeouts.normal, None);
}

#[test]
fn empty_strings_mean_the_automatic_choice() {
    let c = config("[popup]\noutput = \"\"\n[look]\naccent = \"\"\nfont = \"\"\n");
    assert_eq!(c, Config::default());
}

#[test]
fn every_theme_name_parses() {
    for (name, theme) in [
        ("auto", Theme::Auto),
        ("dawn", Theme::Dawn),
        ("daybreak", Theme::Daybreak),
        ("graphite", Theme::Graphite),
        ("Graphite", Theme::Graphite),
    ] {
        assert_eq!(
            config(&format!("theme = \"{name}\"")).theme,
            theme,
            "{name}"
        );
    }
}

#[test]
fn every_position_parses() {
    use Horizontal::*;
    use Vertical::*;
    for (name, vertical, horizontal) in [
        ("top-left", Top, Left),
        ("top-center", Top, Center),
        ("top-right", Top, Right),
        ("bottom-left", Bottom, Left),
        ("bottom-center", Bottom, Center),
        ("bottom-right", Bottom, Right),
    ] {
        let c = config(&format!("[popup]\nposition = \"{name}\""));
        assert_eq!(
            c.popup.position,
            Position {
                vertical,
                horizontal
            },
            "{name}"
        );
    }
}

#[test]
fn the_engine_gets_the_timeouts_stack_size_history_and_dnd_settings() {
    let c = config(
        "[popup]\nmax_visible = 2\n[timeout]\nnormal = 9\n[history]\nlimit = 7\n[dnd]\nallow_critical = false\n",
    );
    let engine = c.engine_config();

    assert_eq!(engine.max_visible, 2);
    assert_eq!(engine.timeouts.normal, Some(Duration::from_secs(9)));
    assert_eq!(engine.timeouts.low, Some(Duration::from_secs(3)));
    assert_eq!(engine.history.limit, 7);
    assert!(!engine.dnd.allow_critical);
}

// ── Warnings ──

const V1_CONFIG: &str = r##"width = 350
timeout_seconds = 5
background_color = "#0D0B14E6"
border_color = "#1A1525"
border_width = 1
text_color = "#E8E2F0"
corner_radius = 12
font_family = "Inter"
max_visible_notifications = 5
"##;

#[test]
fn each_v1_key_points_at_its_v2_replacement() {
    let warnings = warning_lines(V1_CONFIG);

    let expected = [
        "`width` under `[popup]`",
        "`low`, `normal` and `critical` under `[timeout]`",
        "`theme`",
        "`theme` or `style.css`",
        "`style.css`",
        "`theme` or `style.css`",
        "`radius` under `[look]`",
        "`font` under `[look]`",
        "`max_visible` under `[popup]`",
    ];
    assert_eq!(warnings.len(), expected.len(), "{warnings:#?}");
    for ((line, message), (i, replacement)) in warnings.iter().zip(expected.iter().enumerate()) {
        assert_eq!(*line, i + 1);
        assert!(message.contains("v1"), "{message}");
        assert!(
            message.contains(replacement),
            "{message} lacks {replacement}"
        );
        assert!(!message.contains('\n'), "not one line: {message}");
    }
}

#[test]
fn a_whole_v1_config_still_starts_with_the_defaults() {
    assert_eq!(ok(V1_CONFIG).config, Config::default());
}

#[test]
fn unknown_keys_warn_with_their_position_and_are_ignored() {
    let text = "thme = \"dawn\"\n\n[popup]\nwidht = 400\n\n[sparkles]\nlevel = 11\n";
    let parsed = ok(text);

    assert_eq!(parsed.config, Config::default());
    let w = &parsed.warnings;
    assert_eq!(w.len(), 3, "{w:#?}");
    assert_eq!((w[0].line, w[0].column), (1, 1));
    assert!(w[0].message.contains("`thme`"), "{}", w[0].message);
    assert_eq!(w[0].help.as_deref(), Some("did you mean `theme`?"));
    assert_eq!((w[1].line, w[1].column), (4, 1));
    assert!(
        w[1].message.contains("`widht` in `[popup]`"),
        "{}",
        w[1].message
    );
    assert_eq!(w[1].help.as_deref(), Some("did you mean `width`?"));
    assert_eq!(w[2].line, 6);
    assert!(w[2].message.contains("`sparkles`"), "{}", w[2].message);
    assert_eq!(w[2].help, None);
}

#[test]
fn rules_are_left_for_the_rules_feature_without_warnings() {
    config("[[rule]]\napp = \"discord\"\nsummary = \"*is typing*\"\nhide = true\n");
}

// ── Errors ──

#[test]
fn a_value_of_the_wrong_type_is_an_error_at_the_value() {
    let e = error("[popup]\nwidth = \"wide\"\n");
    assert_eq!((e.line, e.column), (2, 9));
    assert!(
        e.message.contains("`width`") && e.message.contains("text"),
        "{}",
        e.message
    );
    assert_eq!(
        e.help.as_deref(),
        Some("write a number without quotes, e.g. `width = 380` (the default)")
    );
}

#[test]
fn an_out_of_range_value_is_an_error() {
    let e = error("[look]\nopacity = 1.5\n");
    assert_eq!((e.line, e.column), (2, 11));
    assert!(e.message.contains("between 0 and 1"), "{}", e.message);

    assert!(
        error("[popup]\nmax_visible = 0")
            .message
            .contains("between 1 and 50")
    );
    assert!(error("[timeout]\nlow = -1").message.contains("seconds"));
    assert!(error("[popup]\ngap = -4").message.contains("pixels"));
}

#[test]
fn an_unknown_theme_suggests_the_closest_name() {
    let e = error("theme = \"graphit\"");
    assert_eq!((e.line, e.column), (1, 9));
    assert_eq!(e.message, "`theme` can't be \"graphit\"");
    let help = e.help.unwrap();
    assert!(
        help.starts_with("did you mean `theme = \"graphite\"`?"),
        "{help}"
    );
    assert!(help.contains("\"daybreak\""), "{help}");
}

#[test]
fn an_invalid_position_lists_the_valid_ones() {
    let e = error("[popup]\nposition = \"middle\"");
    assert_eq!(e.line, 2);
    assert!(e.help.unwrap().contains("\"bottom-center\""));
}

#[test]
fn an_accent_must_be_a_hex_colour() {
    assert!(
        error("[look]\naccent = \"orange\"")
            .message
            .contains("hex colour")
    );
    for accent in ["#fff", "#ff8a3d", "#ff8a3dcc"] {
        config(&format!("[look]\naccent = \"{accent}\""));
    }
}

#[test]
fn booleans_must_be_true_or_false() {
    let e = error("[dnd]\nallow_critical = \"yes\"");
    assert!(e.message.contains("true or false"), "{}", e.message);
}

#[test]
fn a_section_written_as_a_value_is_an_error() {
    let e = error("popup = 3");
    assert!(e.help.unwrap().contains("`[popup]`"));
}

#[test]
fn every_invalid_value_is_reported_in_file_order() {
    let lines: Vec<usize> = errors("[look]\nradius = \"big\"\n[popup]\nwidth = 0\n")
        .iter()
        .map(|e| e.line)
        .collect();
    assert_eq!(lines, [2, 4]);
}

#[test]
fn errors_keep_the_warnings_for_lucent_check() {
    let rejected = parse("thme = 1\n[popup]\nwidth = 0").unwrap_err();
    assert_eq!(rejected.errors.len(), 1);
    assert_eq!(rejected.warnings.len(), 1);
}

#[test]
fn a_toml_syntax_error_has_a_line_column_and_fix() {
    let e = error("[popup]\nwidth = 380\nposition = top-right\n");
    assert_eq!(e.line, 3);
    assert!(e.column >= 12, "{e}");
    assert!(e.help.is_some());
}

#[test]
fn a_diagnostic_prints_its_position_message_and_fix() {
    let printed = error("[popup]\nwidth = \"wide\"\n").to_string();
    assert!(printed.starts_with("2:9: `width` must be"), "{printed}");
    assert!(printed.contains("\n  help: write a number"), "{printed}");
}

// ── The first-run template ──

#[test]
fn the_template_is_entirely_commented_out_and_gives_the_defaults() {
    assert_eq!(config(TEMPLATE), Config::default());
    assert!(
        TEMPLATE
            .lines()
            .all(|l| l.trim().is_empty() || l.starts_with('#'))
    );
}

/// The template with its option lines (`# key = value`, `# [section]`)
/// uncommented.
fn uncommented_template() -> String {
    let is_key = |k: &str| k.chars().all(|c| c.is_ascii_lowercase() || c == '_');
    TEMPLATE
        .lines()
        .map(|line| {
            line.strip_prefix("# ")
                .filter(|rest| {
                    rest.starts_with('[') || rest.split_once(" = ").is_some_and(|(k, _)| is_key(k))
                })
                .unwrap_or(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_template_shows_every_option_with_its_default() {
    let text = uncommented_template();
    assert_eq!(config(&text), Config::default());
    for key in [
        "theme",
        "[popup]",
        "position",
        "width",
        "max_visible",
        "gap",
        "margin",
        "output",
        "[timeout]",
        "low",
        "normal",
        "critical",
        "[look]",
        "accent",
        "opacity",
        "radius",
        "font",
        "icon_size",
        "countdown",
        "[history]",
        "limit",
        "[dnd]",
        "allow_critical",
    ] {
        assert!(
            text.lines().any(|l| l.starts_with(key)),
            "the template doesn't show `{key}`"
        );
    }
}
