//! Round-trip tests against a real `/bin/sh` child, which is guaranteed present
//! wherever these run.

use super::*;
use crate::backend::PaneToken;
use crate::backend::identity::FIRST_GENERATION;
use crate::plugin::protocol::{LogLevel, MAX_LINE_BYTES};

/// Generous like the PTY tests': spawning processes under a parallel test run is
/// slow, and the point of every deadline here is only to fail rather than hang.
const HOST_TEST_DEADLINE: Duration = Duration::from_secs(15);

const POLL: Duration = Duration::from_millis(10);

/// Env var each script reads the command it should echo back from.
const CANNED_ENV: &str = "CANNED";

const CANNED_MESSAGE: &str = "round trip";

fn canned_line() -> String {
    serde_json::to_string(&PluginCommand::Log {
        v: PROTOCOL_VERSION,
        level: LogLevel::Info,
        message: CANNED_MESSAGE.to_string(),
    })
    .expect("encode a command")
}

fn shell_plugin(script: &str) -> PluginConfig {
    PluginConfig {
        name: "test-plugin".to_string(),
        command: "/bin/sh".to_string(),
        args: vec!["-c".to_string(), script.to_string()],
        env: [(CANNED_ENV.to_string(), canned_line())]
            .into_iter()
            .collect(),
        enabled: true,
        ..PluginConfig::default()
    }
}

fn pane_opened() -> PluginEvent {
    PluginEvent::PaneOpened {
        v: PROTOCOL_VERSION,
        token: PaneToken::new().expect("OS RNG"),
        generation: FIRST_GENERATION,
        title: None,
        command: None,
        cwd: "/".to_string(),
    }
}

fn wait_for_command(host: &PluginHost) -> Option<PluginCommand> {
    let deadline = Instant::now() + HOST_TEST_DEADLINE;
    while Instant::now() < deadline {
        if let Some(cmd) = host.try_recv() {
            return Some(cmd);
        }
        thread::sleep(POLL);
    }
    None
}

fn assert_is_canned(cmd: Option<PluginCommand>) {
    match cmd {
        Some(PluginCommand::Log { message, .. }) => assert_eq!(message, CANNED_MESSAGE),
        other => panic!("expected the canned log command, got {other:?}"),
    }
}

#[test]
fn an_event_reaches_the_plugin_and_the_command_it_answers_with_is_decoded() {
    // The command only comes back if the line the plugin read was the event, so
    // receiving it proves both directions.
    let cfg =
        shell_plugin(r#"read line; case "$line" in *pane_opened*) printf '%s\n' "$CANNED";; esac"#);
    let mut host = PluginHost::spawn(&cfg, None).expect("spawn");
    assert!(host.send(&pane_opened()));
    assert_is_canned(wait_for_command(&host));
    host.shutdown();
    assert_eq!(host.dropped_events(), 0);
}

#[test]
fn a_blank_line_from_the_plugin_is_skipped_rather_than_ending_the_stream() {
    let cfg = shell_plugin(r#"printf '\n   \n%s\n' "$CANNED""#);
    let mut host = PluginHost::spawn(&cfg, None).expect("spawn");
    assert_is_canned(wait_for_command(&host));
    host.shutdown();
}

#[test]
fn a_line_the_host_cannot_decode_does_not_stop_later_commands() {
    let cfg = shell_plugin(r#"printf 'not json\n{"cmd":"nope"}\n%s\n' "$CANNED""#);
    let mut host = PluginHost::spawn(&cfg, None).expect("spawn");
    assert_is_canned(wait_for_command(&host));
    host.shutdown();
}

#[test]
fn an_over_long_line_is_discarded_and_the_stream_resynchronises() {
    // 70 KiB in 1000-byte chunks, past MAX_LINE_BYTES, then a valid command:
    // the host must survive the first and still deliver the second.
    const CHUNKS: usize = 70;
    const CHUNK_BYTES: usize = 1000;
    const { assert!(CHUNKS * CHUNK_BYTES > MAX_LINE_BYTES) };
    let cfg = shell_plugin(
        r#"i=0; while [ $i -lt 70 ]; do printf '%01000d' 0; i=$((i+1)); done; printf '\n'; printf '%s\n' "$CANNED""#,
    );
    let mut host = PluginHost::spawn(&cfg, None).expect("spawn");
    assert_is_canned(wait_for_command(&host));
    host.shutdown();
}

#[test]
fn a_plugin_that_exits_at_once_is_reported_not_alive_and_shuts_down_cleanly() {
    let cfg = shell_plugin("exit 0");
    let mut host = PluginHost::spawn(&cfg, None).expect("spawn");
    let deadline = Instant::now() + HOST_TEST_DEADLINE;
    while host.is_alive() && Instant::now() < deadline {
        thread::sleep(POLL);
    }
    assert!(!host.is_alive(), "plugin should have exited");
    host.shutdown();
    assert!(!host.is_alive());
}

#[test]
fn shutting_down_twice_is_safe() {
    let cfg = shell_plugin("cat");
    let mut host = PluginHost::spawn(&cfg, None).expect("spawn");
    assert!(host.is_alive());
    host.shutdown();
    host.shutdown();
    assert!(!host.is_alive());
    // The second call must not have queued anything either.
    assert!(!host.send(&pane_opened()));
}

#[test]
fn a_full_outbound_queue_drops_events_instead_of_blocking() {
    // Depth 1 against a plugin that never reads: the pipe fills, the writer
    // blocks inside its write, and every further event has nowhere to go.
    let cfg = shell_plugin("sleep 30");
    let mut host = PluginHost::spawn_with_queue_depth(&cfg, None, 1).expect("spawn");
    let bulky = PluginEvent::PaneOutput {
        v: PROTOCOL_VERSION,
        token: PaneToken::new().expect("OS RNG"),
        generation: FIRST_GENERATION,
        text: "a".repeat(8 * 1024),
    };

    let deadline = Instant::now() + HOST_TEST_DEADLINE;
    let mut refused = false;
    while !refused && Instant::now() < deadline {
        refused = !host.send(&bulky);
    }

    assert!(refused, "a full queue should have refused an event");
    assert!(host.dropped_events() > 0);
    host.shutdown();
}

#[test]
fn a_command_named_by_path_is_launched_from_that_path() {
    // "/bin/sh" holds a separator, so it is used as given rather than searched
    // for in the plugin directory.
    let cfg = shell_plugin("exit 0");
    let host = PluginHost::spawn(&cfg, Some(Path::new("/nonexistent"))).expect("spawn");
    assert_eq!(host.name(), "test-plugin");
}

#[test]
fn a_plugin_that_cannot_be_launched_is_reported_rather_than_ignored() {
    let mut cfg = shell_plugin("exit 0");
    cfg.command = "/nonexistent/plugin-binary".to_string();
    let Err(err) = PluginHost::spawn(&cfg, None) else {
        panic!("launching a missing binary should have failed");
    };
    assert!(err.to_string().contains("cannot launch plugin"));
}
