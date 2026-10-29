//! Seam 1, config half: config text in, resolved settings and problems out.
//! No files, no logging: `config::load` wraps this with both.

use std::time::Duration;

use lucent::config::{Anchor, ClickAction, Config, General, Layout, Margin, Problem};

fn defaults() -> Config {
    Config {
        general: General {
            timeout: Duration::from_secs(5),
            max_visible: 5,
            idle_exit: true,
            idle_grace: Duration::from_secs(5),
            left_click: ClickAction::InvokeDefaultAction,
            middle_click: ClickAction::Dismiss,
            right_click: ClickAction::DismissAll,
        },
        layout: Layout {
            anchor: Anchor::TopRight,
            margin: Margin {
                top: 16,
                right: 16,
                bottom: 16,
                left: 16,
            },
            width: 360,
            gap: 8,
            monitor: None,
        },
    }
}

#[test]
fn no_config_resolves_to_the_built_in_defaults() {
    assert_eq!(Config::default(), defaults());
    assert_eq!(Config::parse(""), (defaults(), vec![]));
}

#[test]
fn a_partial_config_overrides_only_what_it_sets() {
    let (config, problems) = Config::parse(
        r#"
        [general]
        timeout = 8000

        [layout.margin]
        top = 40
        "#,
    );

    let mut expected = defaults();
    expected.general.timeout = Duration::from_secs(8);
    expected.layout.margin.top = 40;
    assert_eq!(config, expected);
    assert_eq!(problems, []);
}

#[test]
fn a_full_config_sets_every_field() {
    let (config, problems) = Config::parse(
        r#"
        [general]
        timeout = 2500
        max_visible = 3
        idle_exit = false
        idle_grace = 750
        left_click = "dismiss-all"
        middle_click = "none"
        right_click = "invoke-default-action"

        [layout]
        anchor = "bottom-center"
        width = 420
        gap = 12
        monitor = "HDMI-A-1"

        [layout.margin]
        top = 1
        right = 2
        bottom = 3
        left = 4
        "#,
    );

    assert_eq!(
        config,
        Config {
            general: General {
                timeout: Duration::from_millis(2500),
                max_visible: 3,
                idle_exit: false,
                idle_grace: Duration::from_millis(750),
                left_click: ClickAction::DismissAll,
                middle_click: ClickAction::None,
                right_click: ClickAction::InvokeDefaultAction,
            },
            layout: Layout {
                anchor: Anchor::BottomCenter,
                margin: Margin {
                    top: 1,
                    right: 2,
                    bottom: 3,
                    left: 4,
                },
                width: 420,
                gap: 12,
                monitor: Some("HDMI-A-1".into()),
            },
        }
    );
    assert_eq!(problems, []);
}

#[test]
fn every_anchor_has_a_name() {
    for (name, anchor) in [
        ("top-left", Anchor::TopLeft),
        ("top-center", Anchor::TopCenter),
        ("top-centre", Anchor::TopCenter),
        ("top-right", Anchor::TopRight),
        ("bottom-left", Anchor::BottomLeft),
        ("bottom-center", Anchor::BottomCenter),
        ("bottom-centre", Anchor::BottomCenter),
        ("bottom-right", Anchor::BottomRight),
    ] {
        let (config, problems) = Config::parse(&format!("layout.anchor = \"{name}\""));
        assert_eq!((config.layout.anchor, problems), (anchor, vec![]), "{name}");
    }
}

/// A broken file must never silence the desktop: it is reported and the
/// defaults stand in for all of it.
#[test]
fn a_broken_config_is_reported_and_resolves_to_the_defaults() {
    for text in [
        "[general\ntimeout = 3000",                      // not TOML
        "[general]\ntimeout = \"3s\"",                   // wrong type
        "[general]\ntimeout = -1",                       // out of range
        "[general]\ntimeout = 3000\nmax_visible = true", // one bad value spoils the file
        "[layout]\nanchor = \"middle\"",                 // not an anchor
    ] {
        let (config, problems) = Config::parse(text);
        assert_eq!(config, defaults(), "{text:?}");
        assert!(
            matches!(problems.as_slice(), [Problem::Invalid(_)]),
            "{text:?} should be reported as invalid: {problems:?}"
        );
    }
}

#[test]
fn unknown_keys_are_reported_but_not_fatal() {
    let (config, problems) = Config::parse(
        r#"
        colour = "pink"

        [general]
        timeout = 3000
        timeout_seconds = 5

        [layuot]
        width = 1
        "#,
    );

    let mut expected = defaults();
    expected.general.timeout = Duration::from_secs(3);
    assert_eq!(config, expected);
    let mut unknown: Vec<_> = problems
        .iter()
        .map(|p| match p {
            Problem::UnknownKey(key) => key.as_str(),
            other => panic!("only unknown keys expected, got {other:?}"),
        })
        .collect();
    unknown.sort();
    assert_eq!(unknown, ["colour", "general.timeout_seconds", "layuot"]);
}

#[test]
fn lucent_default_config_prints_toml_that_parses_back_to_the_defaults() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lucent"))
        .arg("--default-config")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .output()
        .unwrap();

    assert!(output.status.success(), "exit status {:?}", output.status);
    let printed = String::from_utf8(output.stdout).unwrap();
    assert!(printed.contains("[general]") && printed.contains("[layout]"));
    assert_eq!(Config::parse(&printed), (defaults(), vec![]));
}
