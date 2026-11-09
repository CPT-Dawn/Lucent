//! `lucent osd`: changes a level (volume, microphone, brightness) and shows
//! it as the OSD, in place of a separate OSD daemon such as SwayOSD.
//!
//! The levels are changed through the tools that own them, `wpctl`
//! (WirePlumber) and `brightnessctl`, and shown by sending an ordinary
//! tagged Notification with a `value` hint, which Lucent shows as the OSD
//! (ADR-0004). D-Bus starts Lucent for it if it is not running. Any other
//! daemon that honours the tag shows it as a replacing Notification.

use std::process::Command;
use std::time::Duration;

use dbus::arg::{PropMap, Variant};
use dbus::blocking::Connection;

use crate::config::Config;
use crate::icon::Icons;

pub const USAGE: &str = "\
usage: lucent osd <volume|mic> <up|down> [STEP]
       lucent osd <volume|mic> <mute|show>
       lucent osd brightness <up|down> [STEP]
       lucent osd brightness show

STEP is a percentage, 5 by default. Volume needs wpctl (WirePlumber),
brightness needs brightnessctl.";

/// The level an OSD command acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The default audio output.
    Volume,
    /// The default audio input.
    Mic,
    Brightness,
}

/// What to do to the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Up(u32),
    Down(u32),
    /// Toggle mute; audio only.
    Mute,
    /// Leave it, just show it.
    Show,
}

/// A level, read back after the change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level {
    /// In percent; volume can go above 100 when boosted elsewhere.
    pub percent: u32,
    pub muted: bool,
}

/// The default step, in percent.
const STEP: u32 = 5;

/// Parses the arguments after `osd`.
pub fn parse(args: &[String]) -> Result<(Target, Change), String> {
    let target = match args.first().map(String::as_str) {
        Some("volume") => Target::Volume,
        Some("mic" | "microphone") => Target::Mic,
        Some("brightness") => Target::Brightness,
        Some(other) => return Err(format!("unknown OSD `{other}`")),
        None => return Err("which OSD?".into()),
    };
    let step = || match args.get(2) {
        None => Ok(STEP),
        Some(s) => s
            .trim_end_matches('%')
            .parse::<u32>()
            .ok()
            .filter(|s| (1..=100).contains(s))
            .ok_or_else(|| format!("step `{s}` is not a percentage from 1 to 100")),
    };
    let change = match args.get(1).map(String::as_str) {
        Some("up" | "raise") => Change::Up(step()?),
        Some("down" | "lower") => Change::Down(step()?),
        Some("mute") if target != Target::Brightness => Change::Mute,
        Some("show") => Change::Show,
        Some(other) => return Err(format!("unknown change `{other}`")),
        None => return Err("up, down, mute or show?".into()),
    };
    let extra = match change {
        Change::Up(_) | Change::Down(_) => 3,
        Change::Mute | Change::Show => 2,
    };
    if args.len() > extra {
        return Err(format!("unexpected `{}`", args[extra]));
    }
    Ok((target, change))
}

/// Runs `lucent osd ...`: changes the level, then shows it.
pub fn run(args: &[String]) -> Result<(), String> {
    let (target, change) = parse(args).map_err(|e| format!("{e}\n{USAGE}"))?;
    let level = apply(target, change)?;
    show(target, level)
}

/// Changes the level through the tool that owns it and reads it back.
fn apply(target: Target, change: Change) -> Result<Level, String> {
    match target {
        Target::Volume | Target::Mic => {
            let node = if target == Target::Volume {
                "@DEFAULT_AUDIO_SINK@"
            } else {
                "@DEFAULT_AUDIO_SOURCE@"
            };
            match change {
                // Raising never goes past 100%, as volume keys usually don't,
                // but never pulls down a level boosted past it elsewhere:
                // `-l` caps the result, not the step.
                Change::Up(step) => {
                    let now = volume(node)?.percent;
                    let limit = if now > 100 {
                        format!("{:.2}", f64::from(now) / 100.0)
                    } else {
                        "1.0".to_owned()
                    };
                    wpctl(&["set-volume", "-l", &limit, node, &format!("{step}%+")])?;
                }
                Change::Down(step) => wpctl(&["set-volume", node, &format!("{step}%-")])?,
                Change::Mute => wpctl(&["set-mute", node, "toggle"])?,
                Change::Show => {}
            };
            volume(node)
        }
        Target::Brightness => {
            let mut command = Command::new("brightnessctl");
            command.args(["-m", "--class=backlight"]);
            match change {
                // Never all the way to off, which would blank the screen.
                Change::Down(step) => command.args(["--min-value=1", "set", &format!("{step}%-")]),
                Change::Up(step) => command.args(["set", &format!("{step}%+")]),
                Change::Show | Change::Mute => &mut command,
            };
            let (out, err) = output(&mut command, "brightnessctl")?;
            parse_brightnessctl(&out).ok_or_else(|| unreadable("brightnessctl", &out, &err))
        }
    }
}

/// The volume of `node`, read back from `wpctl get-volume`.
fn volume(node: &str) -> Result<Level, String> {
    let (out, err) = output(Command::new("wpctl").args(["get-volume", node]), "wpctl")?;
    parse_wpctl(&out).ok_or_else(|| unreadable("wpctl", &out, &err))
}

/// Why `name`'s output could not be read. A missing device leaves stdout
/// empty but still exits 0, with the reason on stderr.
fn unreadable(name: &str, out: &str, err: &str) -> String {
    match (out.trim(), err.trim()) {
        ("", "") => format!("{name} printed nothing"),
        ("", err) => format!("{name}: {err}"),
        (out, _) => format!("could not read {name}'s `{out}`"),
    }
}

fn wpctl(args: &[&str]) -> Result<(), String> {
    output(Command::new("wpctl").args(args), "wpctl").map(drop)
}

/// Runs `command`, returning its stdout and stderr, or why it failed.
fn output(command: &mut Command, name: &str) -> Result<(String, String), String> {
    let out = command.output().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!(
            "{name} is not installed; {}",
            if name == "wpctl" {
                "install wireplumber"
            } else {
                "install brightnessctl"
            }
        ),
        _ => format!("could not run {name}: {e}"),
    })?;
    if !out.status.success() {
        return Err(format!(
            "{name} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// Reads `wpctl get-volume`, as `Volume: 0.45` or `Volume: 0.45 [MUTED]`.
pub fn parse_wpctl(out: &str) -> Option<Level> {
    let rest = out.trim().strip_prefix("Volume:")?.trim();
    let mut words = rest.split_whitespace();
    let volume: f64 = words.next()?.parse().ok()?;
    let muted = words.any(|w| w == "[MUTED]");
    Some(Level {
        percent: (volume * 100.0).round().max(0.0) as u32,
        muted,
    })
}

/// Reads `brightnessctl -m`, as `device,class,current,percent%,max`; with
/// several devices, the first line.
pub fn parse_brightnessctl(out: &str) -> Option<Level> {
    let percent = out.lines().next()?.split(',').nth(3)?;
    Some(Level {
        percent: percent.strip_suffix('%')?.parse().ok()?,
        muted: false,
    })
}

/// The icon names for `level` of `target`, best first. Icon themes name
/// these differently, so the first one the user's theme has is sent.
pub fn icons(target: Target, level: Level) -> &'static [&'static str] {
    match target {
        Target::Volume if level.muted || level.percent == 0 => {
            &["audio-volume-muted", "audio-volume-muted-symbolic"]
        }
        Target::Volume if level.percent < 34 => &["audio-volume-low", "audio-volume-low-symbolic"],
        Target::Volume if level.percent < 67 => {
            &["audio-volume-medium", "audio-volume-medium-symbolic"]
        }
        Target::Volume => &["audio-volume-high", "audio-volume-high-symbolic"],
        Target::Mic if level.muted => &[
            "microphone-sensitivity-muted",
            "microphone-sensitivity-muted-symbolic",
            "microphone-disabled-symbolic",
        ],
        Target::Mic => &[
            "microphone-sensitivity-high",
            "microphone-sensitivity-high-symbolic",
            "audio-input-microphone",
            "audio-input-microphone-symbolic",
        ],
        Target::Brightness => &[
            "display-brightness",
            "display-brightness-symbolic",
            "brightness-high",
            "video-display-brightness",
            "brightness-high-symbolic",
        ],
    }
}

/// The first of `names` the configured icon theme has, else the first.
fn icon(names: &[&'static str]) -> &'static str {
    let config = crate::config::path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map_or_else(Config::default, |text| Config::parse(&text).0);
    let theme = config.theme();
    let icons = Icons::new(theme.icon_size, &theme.icon_theme);
    names
        .iter()
        .find(|name| icons.has(name))
        .unwrap_or(&names[0])
}

/// Sends the OSD Notification for `level`.
fn show(target: Target, level: Level) -> Result<(), String> {
    let (summary, tag) = match target {
        Target::Volume => ("Volume", "lucent-osd-volume"),
        Target::Mic => ("Microphone", "lucent-osd-mic"),
        Target::Brightness => ("Brightness", "lucent-osd-brightness"),
    };
    let summary = if level.muted {
        format!("{summary} muted")
    } else {
        format!("{summary} {}%", level.percent)
    };
    let mut hints = PropMap::new();
    hints.insert(
        "x-canonical-private-synchronous".into(),
        Variant(Box::new(tag.to_owned())),
    );
    hints.insert(
        "value".into(),
        Variant(Box::new(i32::try_from(level.percent).unwrap_or(i32::MAX))),
    );
    // Muted levels fade, as low-urgency Popups do.
    let urgency: u8 = if level.muted { 0 } else { 1 };
    hints.insert("urgency".into(), Variant(Box::new(urgency)));

    let conn = Connection::new_session().map_err(|e| format!("no session bus: {e}"))?;
    let proxy = conn.with_proxy(
        crate::dbus::NAME,
        "/org/freedesktop/Notifications",
        // Long enough for D-Bus to start Lucent first.
        Duration::from_secs(10),
    );
    let (_id,): (u32,) = proxy
        .method_call(
            crate::dbus::NAME,
            "Notify",
            (
                "Lucent",
                0u32,
                icon(icons(target, level)),
                summary,
                "",
                Vec::<String>::new(),
                hints,
                -1i32,
            ),
        )
        .map_err(|e| format!("could not show the OSD: {e}"))?;
    Ok(())
}
