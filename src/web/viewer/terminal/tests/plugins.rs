//! The plugin wiring, driven through a real hub and a real plugin child.
//!
//! The fake plugin is `/bin/sh`, which is present wherever these run. It appends
//! every event it is handed to a file the test polls, so what is asserted is what
//! the hub actually sent rather than what it meant to send — and a pane the hub
//! must never mention simply never appears in that file.

use super::{
    attach, collect_created, created_pane, exited_pane, next_matching, reordered_order, spawn_hub,
    wait_for,
};
use crate::config::{PluginConfig, StartupCommand};
use crate::plugin::protocol::PROTOCOL_VERSION;
use crate::web::viewer::terminal::frame::{ClientMessage, PaneSize, TerminalFrame};
use std::path::{Path, PathBuf};

/// Env var the fake plugin appends the events it receives to.
pub(super) const LOG_ENV: &str = "NC_TEST_PLUGIN_LOG";
/// Env var holding the protocol version the fake plugin answers with, so no
/// script hard-codes a number a version bump would silently invalidate.
const VERSION_ENV: &str = "NC_TEST_PLUGIN_V";
/// Env var naming the file the relaunching plugin uses to act exactly once. A
/// puppet that relaunched on every exit would keep relaunching a command that
/// exits immediately, which is the test's own doing rather than the hub's.
const ONCE_ENV: &str = "NC_TEST_PLUGIN_ONCE";

/// A plugin that only records.
fn logging_script() -> String {
    format!(r#"while IFS= read -r line; do printf '%s\n' "$line" >> "${LOG_ENV}"; done"#)
}

/// A plugin that records, then answers the first `pane_exited` with a relaunch
/// of exactly the generation it was told about. `{{`/`}}` are `format!`'s
/// escapes for the JSON braces the command line needs.
fn relaunch_script() -> String {
    format!(
        r#"while IFS= read -r line; do
  printf '%s\n' "$line" >> "${LOG_ENV}"
  case "$line" in
  *pane_exited*)
    if [ ! -f "${ONCE_ENV}" ]; then
      : > "${ONCE_ENV}"
      t=$(printf '%s' "$line" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
      g=$(printf '%s' "$line" | sed -n 's/.*"generation":\([0-9]*\).*/\1/p')
      printf '{{"cmd":"relaunch","v":%s,"token":"%s","generation":%s,"resume_args":[]}}\n' \
        "${VERSION_ENV}" "$t" "$g"
    fi
    ;;
  esac
done"#
    )
}

pub(super) fn shell_plugin(name: &str, script: String, log: &Path, once: &Path) -> PluginConfig {
    PluginConfig {
        name: name.to_string(),
        command: "/bin/sh".to_string(),
        args: vec!["-c".to_string(), script],
        env: [
            (LOG_ENV.to_string(), log.to_string_lossy().into_owned()),
            (ONCE_ENV.to_string(), once.to_string_lossy().into_owned()),
            (VERSION_ENV.to_string(), PROTOCOL_VERSION.to_string()),
        ]
        .into_iter()
        .collect(),
        enabled: true,
        ..PluginConfig::default()
    }
}

/// A plugin that records every event, for a test that only needs to see what the
/// hub sent it.
pub(super) fn recorder(name: &str, log: &Path) -> PluginConfig {
    shell_plugin(name, logging_script(), log, Path::new("/dev/null"))
}

fn pane(name: &str, command: &str, plugin: Option<&str>) -> StartupCommand {
    StartupCommand {
        name: Some(name.to_string()),
        command: command.to_string(),
        plugin: plugin.map(str::to_string),
    }
}

/// Every complete event line the fake plugin has logged so far. A half-written
/// line simply fails to parse and is skipped until the next poll.
pub(super) fn logged(log: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Wait for the plugin to log an event `want` accepts.
pub(super) fn logged_event(
    log: &Path,
    want: impl Fn(&serde_json::Value) -> bool,
) -> Option<serde_json::Value> {
    wait_for(|| logged(log).into_iter().find(|event| want(event)))
}

fn is(event: &serde_json::Value, kind: &str) -> bool {
    event["event"] == kind
}

fn token_of(event: &serde_json::Value) -> String {
    event["token"].as_str().unwrap_or_default().to_string()
}

/// A temp directory plus the paths the fake plugin writes inside it.
pub(super) struct Fixture {
    dir: tempfile::TempDir,
    pub(super) log: PathBuf,
    pub(super) once: PathBuf,
}

pub(super) fn fixture() -> Fixture {
    let dir = tempfile::TempDir::new().unwrap();
    let log = dir.path().join("events.ndjson");
    let once = dir.path().join("relaunched");
    Fixture { dir, log, once }
}

impl Fixture {
    pub(super) fn cwd(&self) -> String {
        self.dir.path().to_string_lossy().into_owned()
    }
}

#[test]
fn a_pane_that_did_not_opt_in_is_never_shown_to_a_plugin() {
    // The plain pane is *first*, so if the hub ever announced it its line would
    // precede the watched pane's on the plugin's single event stream. Seeing the
    // watched pane's `pane_opened` therefore proves the plain one was not
    // announced, rather than merely not announced yet.
    let f = fixture();
    let hub = spawn_hub(
        &f.cwd(),
        vec![
            pane("plain", "printf plain-is-done", None),
            pane("watched", "sleep 30", Some("watch")),
        ],
        vec![recorder("watch", &f.log)],
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&session, 2);

    let first = logged_event(&f.log, |e| is(e, "pane_opened")).expect("no pane_opened logged");
    assert_eq!(
        first["title"], "watched",
        "the first pane announced must be the one that opted in: {first}"
    );

    // And the plain pane's exit is the path it always was: the client is told,
    // and the plugin is not.
    assert!(
        next_matching(&session, |frame| exited_pane(frame) == Some(ids[0])).is_some(),
        "the plain pane's exit must still reach the client"
    );
    let seen: Vec<String> = logged(&f.log).iter().map(|e| e.to_string()).collect();
    assert!(
        !seen.iter().any(|line| line.contains("plain")),
        "a pane with no opt-in must not appear in a plugin's events: {seen:?}"
    );
    hub.stop();
}

#[test]
fn two_opted_in_panes_are_tracked_under_separate_tokens_that_do_not_cross() {
    let f = fixture();
    let hub = spawn_hub(
        &f.cwd(),
        vec![
            pane("alpha", "printf ALPHA-MARK; sleep 30", Some("watch")),
            pane("beta", "printf BETA-MARK; sleep 30", Some("watch")),
        ],
        vec![recorder("watch", &f.log)],
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 24, cols: 80 }; 2],
    });

    let alpha = logged_event(&f.log, |e| is(e, "pane_opened") && e["title"] == "alpha")
        .map(|e| token_of(&e))
        .expect("alpha was not announced");
    let beta = logged_event(&f.log, |e| is(e, "pane_opened") && e["title"] == "beta")
        .map(|e| token_of(&e))
        .expect("beta was not announced");
    assert_ne!(alpha, beta, "two panes must not share a token");

    // Each pane's own output must arrive under its own token, or a plugin
    // watching two panes could not tell which one spoke.
    let carried = |mark: &'static str| {
        logged_event(&f.log, move |e| {
            is(e, "pane_output") && e["text"].as_str().is_some_and(|t| t.contains(mark))
        })
        .map(|e| token_of(&e))
    };
    assert_eq!(carried("ALPHA-MARK"), Some(alpha));
    assert_eq!(carried("BETA-MARK"), Some(beta));
    hub.stop();
}

#[test]
fn a_relaunch_reuses_the_token_advances_the_generation_and_lands_back_at_its_index() {
    // The pane that exits is first in the order, so a replacement left at the
    // end would show up as a wrong order rather than pass unnoticed.
    let f = fixture();
    let hub = spawn_hub(
        &f.cwd(),
        vec![
            pane("recovered", "printf gone; exit 0", Some("watch")),
            pane("other", "sleep 30", None),
        ],
        vec![shell_plugin("watch", relaunch_script(), &f.log, &f.once)],
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&session, 2);

    let opened = logged_event(&f.log, |e| is(e, "pane_opened")).expect("no pane_opened");
    let token = token_of(&opened);
    assert_eq!(opened["generation"], 1);

    let reopened = logged_event(&f.log, |e| is(e, "pane_opened") && e["generation"] == 2)
        .expect("the pane was never relaunched");
    assert_eq!(
        token_of(&reopened),
        token,
        "a relaunch must keep the slot's token, or an observer loses its place"
    );

    // The replacement is a new pane to every client, so it arrives as a create
    // and is then moved back to where its predecessor sat.
    let replacement = next_matching(&session, |frame| {
        created_pane(frame).is_some_and(|id| !ids.contains(&id))
    })
    .and_then(|frame| created_pane(&frame))
    .expect("the replacement pane was never announced");
    let order = next_matching(&session, |frame| reordered_order(frame).is_some())
        .and_then(|frame| reordered_order(&frame))
        .expect("the replacement was not put back in the order");
    assert_eq!(
        order,
        vec![replacement, ids[1]],
        "the relaunched pane must land at its predecessor's index"
    );
    hub.stop();
}

#[test]
fn a_plugin_that_cannot_be_launched_leaves_the_terminal_session_working() {
    let f = fixture();
    let mut broken = recorder("watch", &f.log);
    broken.command = "/nonexistent/nightcrow-test-plugin".to_string();
    let hub = spawn_hub(
        &f.cwd(),
        vec![pane(
            "watched",
            "printf STILL-HERE; sleep 30",
            Some("watch"),
        )],
        vec![broken],
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });

    let created = next_matching(&session, |frame| created_pane(frame).is_some())
        .and_then(|frame| created_pane(&frame))
        .expect("a pane whose plugin failed to launch must still open");
    assert!(
        next_matching(
            &session,
            |frame| matches!(frame, TerminalFrame::Output { pane, data }
            if *pane == created && String::from_utf8_lossy(data).contains("STILL-HERE"))
        )
        .is_some(),
        "the pane must still stream its output"
    );

    // And the hub keeps serving: a client can still open another terminal.
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    assert!(
        next_matching(&session, |frame| created_pane(frame)
            .is_some_and(|id| id != created))
        .is_some(),
        "the hub stopped serving after a plugin failed to launch"
    );
    hub.stop();
}
