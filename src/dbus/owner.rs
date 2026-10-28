//! Who owns `org.freedesktop.Notifications` when it isn't us, and what to
//! tell the user about it.

/// What can be learned about the process that owns the notification name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Owner {
    /// The name from its `GetServerInformation`, e.g. "mako".
    pub server_name: Option<String>,
    /// Its process name (`/proc/<pid>/comm`), e.g. "swaync".
    pub process: Option<String>,
    pub pid: Option<u32>,
    /// The systemd user service it runs as, if it has a real one.
    pub unit: Option<String>,
}

impl Owner {
    pub fn is_lucent(&self) -> bool {
        self.server_name.as_deref() == Some("Lucent")
    }

    /// "mako is", "Another notification daemon is", ...
    fn is(&self) -> String {
        match self.process.as_deref().or(self.server_name.as_deref()) {
            Some(name) => format!("{name} is"),
            None => "Another notification daemon is".into(),
        }
    }

    fn pid_note(&self) -> String {
        self.pid
            .map(|pid| format!(" (pid {pid})"))
            .unwrap_or_default()
    }
}

/// What `lucent` prints, then exits successfully, when another daemon
/// owns the notification name.
pub fn standing_aside(owner: &Owner) -> String {
    if owner.is_lucent() {
        return format!(
            "Lucent is already running{}. Try `lucent demo` to see it shine.",
            owner.pid_note()
        );
    }
    format!(
        "{} already showing your Notifications{}, so Lucent is standing aside.\n{}",
        owner.is(),
        owner.pid_note(),
        switch_steps(owner)
    )
}

/// What `lucent <subcommand>` adds when Lucent isn't running because
/// another daemon has the notification name.
pub fn instead_of_lucent(owner: &Owner) -> String {
    format!(
        "{} handling Notifications instead.\n{}",
        owner.is(),
        switch_steps(owner)
    )
}

/// Desktops whose shell itself serves Notifications.
const DESKTOP_SHELLS: [&str; 3] = ["gnome-shell", "plasmashell", "cinnamon"];

/// How to hand the notification name over to Lucent.
fn switch_steps(owner: &Owner) -> String {
    let process = owner.process.as_deref();
    if let Some(shell) = process.filter(|p| DESKTOP_SHELLS.contains(p)) {
        return format!(
            "It's built into your desktop ({shell}), so it can't be stopped on its own; \
             Lucent is meant for compositors like Hyprland, Sway and niri.\n"
        );
    }
    match (&owner.unit, process) {
        (Some(unit), _) => format!(
            "To switch to Lucent for good, stop the service and start Lucent:\n\
             \n    systemctl --user disable --now {unit} && lucent\n"
        ),
        (None, Some(process)) => format!(
            "To switch to Lucent, stop it and start Lucent:\n\
             \n    pkill -x {process} && lucent\n\
             \nThen take {process} out of your autostart (e.g. `exec-once` in \
             hyprland.conf) so it stays stopped.\n"
        ),
        (None, None) => "To find out which program it is, run:\n\
             \n    busctl --user status org.freedesktop.Notifications\n\
             \nStop that program, then run `lucent`.\n"
            .into(),
    }
}

/// The systemd user service in a `/proc/<pid>/cgroup` file, if the
/// process runs as a real one. Transient units (apps launched by the
/// session, D-Bus activation) don't count: disabling them does nothing.
pub fn systemd_unit(cgroup: &str) -> Option<String> {
    let path = cgroup.lines().find_map(|line| line.strip_prefix("0::"))?;
    let unit = path.rsplit('/').next()?;
    let transient = unit.starts_with("app-") || unit.starts_with("dbus-") || unit.contains('@');
    (unit.ends_with(".service") && !transient).then(|| unit.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(name: &str) -> Owner {
        Owner {
            process: Some(name.into()),
            pid: Some(4242),
            ..Owner::default()
        }
    }

    #[test]
    fn mako_is_named_with_a_way_to_stop_it() {
        let text = standing_aside(&process("mako"));
        assert!(
            text.starts_with("mako is already showing your Notifications (pid 4242)"),
            "{text}"
        );
        assert!(text.contains("pkill -x mako && lucent"), "{text}");
        assert!(text.contains("autostart"), "{text}");
    }

    #[test]
    fn a_systemd_service_is_disabled_rather_than_killed() {
        let owner = Owner {
            unit: Some("dunst.service".into()),
            ..process("dunst")
        };
        let text = standing_aside(&owner);
        assert!(text.starts_with("dunst is already"), "{text}");
        assert!(
            text.contains("systemctl --user disable --now dunst.service && lucent"),
            "{text}"
        );
        assert!(!text.contains("pkill"), "{text}");
    }

    #[test]
    fn the_server_name_names_it_when_the_process_is_unknown() {
        let owner = Owner {
            server_name: Some("fnott".into()),
            ..Owner::default()
        };
        assert!(standing_aside(&owner).starts_with("fnott is already showing"));
    }

    #[test]
    fn an_unknown_daemon_gets_a_way_to_find_it() {
        let text = standing_aside(&Owner::default());
        assert!(
            text.starts_with("Another notification daemon is already showing"),
            "{text}"
        );
        assert!(
            text.contains("busctl --user status org.freedesktop.Notifications"),
            "{text}"
        );
    }

    #[test]
    fn a_running_lucent_is_not_a_rival() {
        let owner = Owner {
            server_name: Some("Lucent".into()),
            ..process("lucent")
        };
        assert_eq!(
            standing_aside(&owner),
            "Lucent is already running (pid 4242). Try `lucent demo` to see it shine."
        );
    }

    #[test]
    fn desktop_shells_are_explained_not_killed() {
        let text = standing_aside(&process("gnome-shell"));
        assert!(text.contains("built into your desktop"), "{text}");
        assert!(!text.contains("pkill"), "{text}");
    }

    #[test]
    fn subcommands_say_who_is_in_lucents_place() {
        let text = instead_of_lucent(&process("swaync"));
        assert!(
            text.starts_with("swaync is handling Notifications instead."),
            "{text}"
        );
        assert!(text.contains("pkill -x swaync && lucent"), "{text}");
    }

    #[test]
    fn only_real_user_services_count_as_units() {
        let slice = "0::/user.slice/user-1000.slice/user@1000.service/app.slice";
        assert_eq!(
            systemd_unit(&format!("{slice}/swaync.service\n")),
            Some("swaync.service".into())
        );
        assert_eq!(
            systemd_unit(&format!("{slice}/app-Hyprland-mako@x1.service")),
            None
        );
        assert_eq!(
            systemd_unit(&format!(
                "{slice}/dbus-:1.2-org.freedesktop.Notifications@0.service"
            )),
            None
        );
        assert_eq!(
            systemd_unit("0::/user.slice/user-1000.slice/session-2.scope"),
            None
        );
        assert_eq!(systemd_unit(""), None);
    }
}
