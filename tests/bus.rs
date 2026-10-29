//! Seam 2: the D-Bus adapter. Runs the real binary on a private `dbus-daemon`
//! with drawing switched off and talks to it like an application would.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use dbus::arg::PropMap;
use dbus::blocking::{Connection, Proxy};
use dbus::message::MatchRule;

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const CALL_TIMEOUT: Duration = Duration::from_secs(2);

/// A private session bus, killed on drop, with its own config home so the
/// user's real config never leaks into a test.
struct Bus {
    daemon: Child,
    addr: String,
    config_home: tempfile::TempDir,
    /// `$XDG_RUNTIME_DIR` for Lucent, where it persists the last id.
    runtime_dir: tempfile::TempDir,
}

impl Bus {
    fn new() -> Bus {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon must be in PATH");
        let mut addr = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut addr)
            .unwrap();
        Bus {
            daemon,
            addr: addr.trim().to_owned(),
            config_home: tempfile::tempdir().unwrap(),
            runtime_dir: tempfile::tempdir().unwrap(),
        }
    }

    /// A private session bus that starts Lucent by D-Bus activation, the way
    /// a desktop session does, with drawing off and `config` as its config.
    fn activating(config: &str) -> Bus {
        let config_home = tempfile::tempdir().unwrap();
        let runtime_dir = tempfile::tempdir().unwrap();
        let lucent_config = config_home.path().join("lucent");
        std::fs::create_dir_all(&lucent_config).unwrap();
        std::fs::write(lucent_config.join("config.toml"), config).unwrap();

        let services = runtime_dir.path().join("services");
        std::fs::create_dir_all(&services).unwrap();
        std::fs::write(
            services.join(format!("{NAME}.service")),
            format!(
                "[D-BUS Service]\nName={NAME}\nExec={}\n",
                env!("CARGO_BIN_EXE_lucent")
            ),
        )
        .unwrap();
        let bus_config = runtime_dir.path().join("bus.conf");
        std::fs::write(
            &bus_config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:dir={dir}</listen>
  <servicedir>{services}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                dir = runtime_dir.path().display(),
                services = services.display(),
            ),
        )
        .unwrap();

        // Activated services inherit the daemon's environment.
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", bus_config.display()))
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .env("LUCENT_NO_DRAW", "1")
            .env("XDG_CONFIG_HOME", config_home.path())
            .env("XDG_RUNTIME_DIR", runtime_dir.path())
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("LUCENT_LOG")
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon must be in PATH");
        let mut addr = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut addr)
            .unwrap();
        Bus {
            daemon,
            addr: addr.trim().to_owned(),
            config_home,
            runtime_dir,
        }
    }

    /// Gives Lucents started after this call `text` as their config file.
    fn write_config(&self, text: &str) {
        let dir = self.config_home.path().join("lucent");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), text).unwrap();
    }

    fn client(&self) -> Connection {
        let mut ch = dbus::channel::Channel::open_private(&self.addr).unwrap();
        ch.register().unwrap();
        Connection::from(ch)
    }

    fn lucent_command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_lucent"));
        cmd.env("DBUS_SESSION_BUS_ADDRESS", &self.addr)
            .env("LUCENT_NO_DRAW", "1")
            .env("XDG_CONFIG_HOME", self.config_home.path())
            .env("XDG_RUNTIME_DIR", self.runtime_dir.path())
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("LUCENT_LOG");
        cmd
    }

    /// Starts Lucent and waits until it owns the bus name.
    fn spawn_lucent(&self) -> Lucent {
        let child = self.lucent_command().spawn().unwrap();
        let lucent = Lucent(child);
        let client = self.client();
        let start = Instant::now();
        while !name_has_owner(&client, NAME) {
            assert!(
                start.elapsed() < Duration::from_secs(5),
                "Lucent never took the bus name"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        lucent
    }
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// The Lucent process under test, killed on drop.
struct Lucent(Child);

impl Drop for Lucent {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn name_has_owner(client: &Connection, name: &str) -> bool {
    let bus = client.with_proxy(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        CALL_TIMEOUT,
    );
    let (has,): (bool,) = bus
        .method_call("org.freedesktop.DBus", "NameHasOwner", (name,))
        .unwrap();
    has
}

fn notifications(client: &Connection) -> Proxy<'_, &Connection> {
    client.with_proxy(NAME, PATH, CALL_TIMEOUT)
}

fn notify(client: &Connection, summary: &str, body: &str) -> u32 {
    notify_replacing(client, 0, summary, body)
}

fn notify_replacing(client: &Connection, replaces_id: u32, summary: &str, body: &str) -> u32 {
    let (id,): (u32,) = notifications(client)
        .method_call(
            NAME,
            "Notify",
            (
                "test-app",
                replaces_id,
                "",
                summary,
                body,
                Vec::<String>::new(),
                PropMap::new(),
                -1i32,
            ),
        )
        .unwrap();
    id
}

#[test]
fn reports_server_information_and_capabilities_and_accepts_notify() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();

    let (name, vendor, version, spec): (String, String, String, String) = notifications(&client)
        .method_call(NAME, "GetServerInformation", ())
        .unwrap();
    assert_eq!(
        (name.as_str(), vendor.as_str(), spec.as_str()),
        ("Lucent", "CPT-Dawn", "1.3")
    );
    assert_eq!(version, env!("CARGO_PKG_VERSION"));

    let (caps,): (Vec<String>,) = notifications(&client)
        .method_call(NAME, "GetCapabilities", ())
        .unwrap();
    assert_eq!(
        caps,
        [
            "actions",
            "body",
            "body-hyperlinks",
            "body-markup",
            "icon-static"
        ]
    );

    let first = notify(&client, "Hello", "World");
    let second = notify(&client, "Hello", "again");
    assert!(first > 0, "ids start above zero");
    assert_ne!(first, second, "every Notify gets its own id");
}

/// Collects `NotificationClosed` signals until `deadline` has passed.
fn closed_signals_until(
    client: &Connection,
    start: Instant,
    deadline: Duration,
) -> Vec<((u32, u32), Duration)> {
    let mut closed = Vec::new();
    while start.elapsed() < deadline {
        if let Some(msg) = client
            .channel()
            .blocking_pop_message(Duration::from_millis(100))
            .unwrap()
            && msg.member().as_deref() == Some("NotificationClosed")
        {
            closed.push((msg.read2::<u32, u32>().unwrap(), start.elapsed()));
        }
    }
    closed
}

#[test]
fn an_unattended_notification_expires_with_exactly_one_closed_signal() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();
    client
        .add_match_no_cb(&MatchRule::new_signal(NAME, "NotificationClosed").match_str())
        .unwrap();

    let id = notify(&client, "Hello", "World");

    // The default timeout is 5 s; listen a little past it for duplicates.
    let closed = closed_signals_until(&client, Instant::now(), Duration::from_millis(6500));

    assert_eq!(
        closed.len(),
        1,
        "exactly one NotificationClosed: {closed:?}"
    );
    let ((closed_id, reason), at) = closed[0];
    assert_eq!((closed_id, reason), (id, 1), "reason 1 means expired");
    assert!(
        at >= Duration::from_millis(4900),
        "closed too early, after {at:?}"
    );
}

#[test]
fn serves_notifications_on_a_single_thread() {
    let bus = Bus::new();
    let lucent = bus.spawn_lucent();
    let client = bus.client();
    notify(&client, "Hello", "World");

    // With drawing off Pango never starts its fontconfig helper, so this
    // counts Lucent's own threads: no runtime, no executor, no workers.
    let tasks = std::fs::read_dir(format!("/proc/{}/task", lucent.0.id()))
        .unwrap()
        .count();
    assert_eq!(tasks, 1);
}

#[test]
fn names_the_daemon_holding_the_bus_name_and_exits_cleanly() {
    let bus = Bus::new();
    let first = bus.spawn_lucent();

    let output = bus.lucent_command().output().unwrap();

    assert!(output.status.success(), "exit status {:?}", output.status);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let first_pid = first.0.id();
    assert!(
        stderr.contains(&format!("lucent (pid {first_pid})")),
        "stderr should name the owner: {stderr}"
    );
}

#[test]
fn logs_only_errors_by_default_and_everything_with_lucent_log_debug() {
    let bus = Bus::new();
    let client = bus.client();

    for (log, expect_debug) in [(None, false), (Some("debug"), true)] {
        let mut cmd = bus.lucent_command();
        if let Some(level) = log {
            cmd.env("LUCENT_LOG", level);
        }
        let mut child = cmd.stderr(Stdio::piped()).spawn().unwrap();
        while !name_has_owner(&client, NAME) {
            std::thread::sleep(Duration::from_millis(10));
        }
        notify(&client, "Hello", "World");
        child.kill().unwrap();
        let output = child.wait_with_output().unwrap();
        while name_has_owner(&client, NAME) {
            std::thread::sleep(Duration::from_millis(10));
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if expect_debug {
            assert!(stderr.contains("Notify"), "debug log: {stderr}");
        } else {
            assert!(stderr.is_empty(), "default log should be quiet: {stderr}");
        }
    }
}

#[test]
fn an_image_data_hint_survives_the_wire() {
    let bus = Bus::new();
    let client = bus.client();
    let mut child = bus
        .lucent_command()
        .env("LUCENT_LOG", "debug")
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    while !name_has_owner(&client, NAME) {
        std::thread::sleep(Duration::from_millis(10));
    }

    let mut hints = PropMap::new();
    let pixels = (2i32, 1i32, 6i32, false, 8i32, 3i32, vec![0u8; 6]);
    hints.insert("image-data".into(), dbus::arg::Variant(Box::new(pixels)));
    let (_id,): (u32,) = notifications(&client)
        .method_call(
            NAME,
            "Notify",
            (
                "test-app",
                0u32,
                "firefox",
                "Look",
                "",
                Vec::<String>::new(),
                hints,
                -1i32,
            ),
        )
        .unwrap();
    child.kill().unwrap();
    let stderr = String::from_utf8_lossy(&child.wait_with_output().unwrap().stderr).into_owned();

    assert!(stderr.contains("Pixels(2x1)"), "debug log: {stderr}");
}

#[test]
fn the_config_file_sets_the_timeout() {
    let bus = Bus::new();
    bus.write_config("[general]\ntimeout = 300\n");
    let _lucent = bus.spawn_lucent();
    let client = bus.client();
    client
        .add_match_no_cb(&MatchRule::new_signal(NAME, "NotificationClosed").match_str())
        .unwrap();

    let id = notify(&client, "Hello", "World");
    let closed = closed_signals_until(&client, Instant::now(), Duration::from_millis(1500));

    assert_eq!(closed.len(), 1, "{closed:?}");
    let ((closed_id, reason), at) = closed[0];
    assert_eq!((closed_id, reason), (id, 1));
    assert!(
        (Duration::from_millis(250)..Duration::from_millis(1200)).contains(&at),
        "expired after {at:?}, configured 300 ms"
    );
}

#[test]
fn a_broken_config_is_logged_and_notifications_still_show() {
    let bus = Bus::new();
    bus.write_config("[general]\ntimeout = \"soon\"\n");
    let client = bus.client();

    let mut child = bus.lucent_command().stderr(Stdio::piped()).spawn().unwrap();
    let start = Instant::now();
    while !name_has_owner(&client, NAME) {
        assert!(start.elapsed() < Duration::from_secs(5), "never served");
        std::thread::sleep(Duration::from_millis(10));
    }
    let id = notify(&client, "Hello", "World");
    child.kill().unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(id > 0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("error") && stderr.contains("config.toml"),
        "the broken file should be named in the log: {stderr}"
    );
}

fn close_notification(client: &Connection, id: u32) {
    let () = notifications(client)
        .method_call(NAME, "CloseNotification", (id,))
        .unwrap();
}

fn listen_for_closed(client: &Connection) {
    client
        .add_match_no_cb(&MatchRule::new_signal(NAME, "NotificationClosed").match_str())
        .unwrap();
}

#[test]
fn close_notification_emits_exactly_one_closed_signal_with_reason_3() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();
    listen_for_closed(&client);

    let id = notify(&client, "Hello", "World");
    close_notification(&client, id);

    // Listen well past the default timeout: the expiry must not fire too.
    let closed = closed_signals_until(&client, Instant::now(), Duration::from_millis(6000));
    let signals: Vec<_> = closed.iter().map(|(signal, _)| *signal).collect();
    assert_eq!(signals, [(id, 3)], "reason 3 means closed by a call");
}

#[test]
fn closing_an_unknown_id_emits_nothing() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();
    listen_for_closed(&client);

    close_notification(&client, 4242);

    let closed = closed_signals_until(&client, Instant::now(), Duration::from_millis(500));
    assert_eq!(closed, []);
}

#[test]
fn a_replacement_keeps_the_id_and_an_unknown_replaces_id_gets_a_fresh_one() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();

    let id = notify(&client, "volume", "40%");
    assert_eq!(notify_replacing(&client, id, "volume", "45%"), id);

    let fresh = notify_replacing(&client, 4242, "stranger", "hi");
    assert!(fresh != id && fresh != 4242, "got {fresh}");
}

/// Waits for Lucent to end on its own and returns how long that took.
fn wait_for_exit(lucent: &mut Lucent, within: Duration) -> Duration {
    let start = Instant::now();
    loop {
        if let Some(status) = lucent.0.try_wait().unwrap() {
            assert!(status.success(), "exit status {status:?}");
            return start.elapsed();
        }
        assert!(start.elapsed() < within, "Lucent is still running");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn lucent_exits_once_its_last_popup_closes_and_the_grace_period_passes() {
    let bus = Bus::new();
    bus.write_config("[general]\ntimeout = 100\nidle_grace = 400\n");
    let mut lucent = bus.spawn_lucent();
    let client = bus.client();

    notify(&client, "Hello", "World");
    let took = wait_for_exit(&mut lucent, Duration::from_secs(3));

    assert!(took >= Duration::from_millis(450), "exited after {took:?}");
    assert!(!name_has_owner(&client, NAME), "the bus name was released");
}

#[test]
fn with_idle_exit_off_lucent_stays_resident() {
    let bus = Bus::new();
    bus.write_config("[general]\ntimeout = 100\nidle_exit = false\nidle_grace = 100\n");
    let mut lucent = bus.spawn_lucent();
    let client = bus.client();

    notify(&client, "Hello", "World");
    std::thread::sleep(Duration::from_millis(800));

    assert!(lucent.0.try_wait().unwrap().is_none(), "Lucent exited");
}

#[test]
fn ids_keep_increasing_across_restarts() {
    let bus = Bus::new();
    bus.write_config("[general]\ntimeout = 50\nidle_grace = 50\n");
    let client = bus.client();

    let mut lucent = bus.spawn_lucent();
    let first = notify(&client, "Hello", "World");
    let second = notify(&client, "Hello", "again");
    wait_for_exit(&mut lucent, Duration::from_secs(3));

    let _lucent = bus.spawn_lucent();
    let after_restart = notify(&client, "Hello", "from the other side");

    assert!(
        first < second && second < after_restart,
        "ids {first}, {second}, then {after_restart} after the restart"
    );
}

#[test]
fn notifications_sent_while_lucent_shuts_down_are_answered_or_reactivate_it() {
    // Popups vanish within a millisecond and Lucent exits right after, so a stream of
    // Notifications, sent at a sweep of delays, keeps landing on a Lucent
    // that is in the middle of shutting down.
    let bus = Bus::activating("[general]\ntimeout = 1\nidle_grace = 0\n");
    let client = bus.client();
    let rule = MatchRule::new_signal("org.freedesktop.DBus", "NameOwnerChanged")
        .with_sender("org.freedesktop.DBus");
    client.add_match_no_cb(&rule.match_str()).unwrap();

    let mut ids = Vec::new();
    // The window is narrow, so this catches a regression often, not always.
    for i in 0..400u64 {
        // A Notify left unanswered by an exiting Lucent makes `notify` panic.
        ids.push(notify(&client, "tick", &i.to_string()));
        std::thread::sleep(Duration::from_micros(i * 37 % 1500));
    }

    let mut activations = 0;
    while let Some(msg) = client.channel().pop_message() {
        if msg.member().as_deref() == Some("NameOwnerChanged")
            && let Ok((name, _, new)) = msg.read3::<String, String, String>()
            && name == NAME
            && !new.is_empty()
        {
            activations += 1;
        }
    }
    assert!(
        ids.windows(2).all(|w| w[0] < w[1]),
        "ids must keep increasing: {ids:?}"
    );
    assert!(
        activations > 1,
        "Lucent should have exited and been reactivated along the way"
    );
}

#[test]
#[cfg(not(feature = "test-hooks"))]
fn without_the_test_hooks_feature_the_test_interface_does_not_exist() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();
    let id = notify(&client, "Hello", "");

    let click: Result<(), _> =
        notifications(&client).method_call("dev.cptdawn.Lucent.Test", "Click", (id, "left"));

    assert_eq!(
        click.unwrap_err().name(),
        Some("org.freedesktop.DBus.Error.UnknownMethod")
    );
}

#[test]
#[cfg(feature = "test-hooks")]
fn clicking_a_notification_with_a_default_action_invokes_it_then_dismisses() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();
    listen_for_closed(&client);
    client
        .add_match_no_cb(&MatchRule::new_signal(NAME, "ActionInvoked").match_str())
        .unwrap();

    let (id,): (u32,) = notifications(&client)
        .method_call(
            NAME,
            "Notify",
            (
                "mail",
                0u32,
                "",
                "New mail",
                "",
                vec!["default", "Open"],
                PropMap::new(),
                -1i32,
            ),
        )
        .unwrap();
    // With drawing off there is no pointer; this test-only call stands in.
    let () = notifications(&client)
        .method_call("dev.cptdawn.Lucent.Test", "Click", (id, "left"))
        .unwrap();

    let mut signals = Vec::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        let Some(msg) = client
            .channel()
            .blocking_pop_message(Duration::from_millis(50))
            .unwrap()
        else {
            continue;
        };
        match msg.member().as_deref() {
            Some("ActionInvoked") => {
                let (id, key) = msg.read2::<u32, String>().unwrap();
                signals.push(format!("ActionInvoked({id}, {key})"));
            }
            Some("NotificationClosed") => {
                let (id, reason) = msg.read2::<u32, u32>().unwrap();
                signals.push(format!("NotificationClosed({id}, {reason})"));
            }
            _ => {}
        }
    }
    assert_eq!(
        signals,
        [
            format!("ActionInvoked({id}, default)"),
            format!("NotificationClosed({id}, 2)"),
        ]
    );
}

#[test]
#[cfg(feature = "test-hooks")]
fn pressing_an_action_invokes_its_key_then_dismisses() {
    let bus = Bus::new();
    let _lucent = bus.spawn_lucent();
    let client = bus.client();
    listen_for_closed(&client);
    client
        .add_match_no_cb(&MatchRule::new_signal(NAME, "ActionInvoked").match_str())
        .unwrap();

    let (id,): (u32,) = notifications(&client)
        .method_call(
            NAME,
            "Notify",
            (
                "mail",
                0u32,
                "",
                "New mail",
                "",
                vec!["default", "Open", "reply", "Reply"],
                PropMap::new(),
                -1i32,
            ),
        )
        .unwrap();
    // With drawing off there are no Actions on screen; this test-only call stands in.
    let () = notifications(&client)
        .method_call("dev.cptdawn.Lucent.Test", "InvokeAction", (id, "reply"))
        .unwrap();

    let mut signals = Vec::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(500) {
        let Some(msg) = client
            .channel()
            .blocking_pop_message(Duration::from_millis(50))
            .unwrap()
        else {
            continue;
        };
        match msg.member().as_deref() {
            Some("ActionInvoked") => {
                let (id, key) = msg.read2::<u32, String>().unwrap();
                signals.push(format!("ActionInvoked({id}, {key})"));
            }
            Some("NotificationClosed") => {
                let (id, reason) = msg.read2::<u32, u32>().unwrap();
                signals.push(format!("NotificationClosed({id}, {reason})"));
            }
            _ => {}
        }
    }
    assert_eq!(
        signals,
        [
            format!("ActionInvoked({id}, reply)"),
            format!("NotificationClosed({id}, 2)"),
        ]
    );
}
