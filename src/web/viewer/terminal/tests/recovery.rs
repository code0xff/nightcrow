//! What a person sees of a plugin's recovery, and what cancelling it does.
//!
//! Driven through a real hub and a real plugin child, like `plugins.rs`: what is
//! asserted is the frame that actually left the hub, not what it meant to send.

use super::plugin_rules::{COLS, LONG_RUNNING, PLUGIN, ROWS, opt_in, token_of};
use super::plugins::{Fixture, fixture, logged_event, shell_plugin};
use super::{collect_created, created_pane, next_matching};
use crate::backend::{PaneId, PtyBackend};
use crate::config::{PluginConfig, StartupCommand};
use crate::web::viewer::terminal::frame::{ClientMessage, TerminalFrame};
use crate::web::viewer::terminal::hub_plugins::Plugins;
use crate::web::viewer::terminal::hub_plugins_slots::{PENDING_RELAUNCH_TTL, PaneSpot};
use crate::web::viewer::terminal::hub_recovery::RECOVERY_CANCELLED;
use crate::web::viewer::terminal::{TerminalHub, TerminalSession};
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a test watches for a frame it expects *not* to arrive. Long enough
/// for the 8 ms worker loop to have drained the cancel it was given several
/// hundred times over, and short enough that proving a negative is cheap.
const QUIET_WINDOW: Duration = Duration::from_millis(500);

/// The state and detail the fake plugin reports, so the assertions name the same
/// strings the script prints.
const REPORTED_STATE: &str = "waiting_for_reset";
const REPORTED_DETAIL: &str = "provider window closed";
const REPORTED_DEADLINE: i64 = 1_700_000_000;
const REPORTED_ATTEMPT: u32 = 2;

/// A plugin that answers the pane it is given with one status report.
///
/// Once, on `pane_opened`: a report per event would make "the client saw one"
/// unfalsifiable, and the point is that a single report reaches every client.
fn status_script() -> String {
    format!(
        r#"while IFS= read -r line; do
  printf '%s\n' "$line" >> "${{NC_TEST_PLUGIN_LOG}}"
  case "$line" in
  *pane_opened*)
    t=$(printf '%s' "$line" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
    g=$(printf '%s' "$line" | sed -n 's/.*"generation":\([0-9]*\).*/\1/p')
    printf '{{"cmd":"status","v":%s,"token":"%s","generation":%s,"state":"{REPORTED_STATE}","detail":"{REPORTED_DETAIL}","deadline_epoch":{REPORTED_DEADLINE},"attempt":{REPORTED_ATTEMPT}}}\n' \
      "${{NC_TEST_PLUGIN_V}}" "$t" "$g"
    ;;
  esac
done"#
    )
}

fn reporter(name: &str, f: &Fixture) -> PluginConfig {
    shell_plugin(name, status_script(), &f.log, Path::new("/dev/null"))
}

fn watched(name: &str, command: &str) -> StartupCommand {
    StartupCommand {
        name: Some(name.to_string()),
        command: command.to_string(),
        plugin: Some(PLUGIN.to_string()),
    }
}

/// One recovery report as it came off the wire.
#[derive(Debug, PartialEq, Eq)]
struct Report {
    pane: PaneId,
    state: String,
    detail: Option<String>,
    deadline_epoch: Option<i64>,
    attempt: u32,
}

/// The whole recovery report a frame carries, or `None` for any other frame.
fn recovery(frame: &TerminalFrame) -> Option<Report> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "recovery" {
        return None;
    }
    Some(Report {
        pane: value["pane"].as_u64()? as PaneId,
        state: value["state"].as_str()?.to_string(),
        detail: value["detail"].as_str().map(str::to_string),
        deadline_epoch: value["deadline_epoch"].as_i64(),
        attempt: value["attempt"].as_u64()? as u32,
    })
}

#[test]
fn a_plugin_status_report_reaches_a_client_as_a_recovery_frame() {
    let f = fixture();
    let hub = TerminalHub::spawn(
        &f.cwd(),
        vec![watched("watched", LONG_RUNNING)],
        vec![reporter(PLUGIN, &f)],
    );
    let session = hub.connect();
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&session, 1);

    let report = next_matching(&session, |frame| recovery(frame).is_some())
        .and_then(|frame| recovery(&frame))
        .expect("the plugin's report never reached the client");

    assert_eq!(
        report,
        Report {
            pane: ids[0],
            state: REPORTED_STATE.to_string(),
            detail: Some(REPORTED_DETAIL.to_string()),
            deadline_epoch: Some(REPORTED_DEADLINE),
            attempt: REPORTED_ATTEMPT,
        },
        "the report must reach the client exactly as the plugin gave it"
    );
    hub.stop();
}

#[test]
fn cancelling_a_recovery_releases_the_hold_and_tells_every_client() {
    // Two clients, because the point of broadcasting `cancelled` is that the one
    // who did not press the key stops showing a deadline too.
    let f = fixture();
    let hub = TerminalHub::spawn(
        &f.cwd(),
        vec![watched("watched", "printf gone; exit 0")],
        vec![reporter(PLUGIN, &f)],
    );
    let presser = hub.connect();
    let watcher = hub.connect();
    presser.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&presser, 1);

    // The hold exists once the plugin has been told its pane exited — that event
    // is sent on the same path that creates it.
    assert!(
        logged_event(&f.log, |e| e["event"] == "pane_exited").is_some(),
        "the pane never exited into a hold"
    );

    presser.dispatch(ClientMessage::CancelRecovery { pane: ids[0] });

    for (who, session) in [("the presser", &presser), ("the other client", &watcher)] {
        let report = next_matching(session, |frame| {
            recovery(frame).is_some_and(|r| r.state == RECOVERY_CANCELLED)
        })
        .and_then(|frame| recovery(&frame))
        .unwrap_or_else(|| panic!("{who} was never told the recovery was cancelled"));
        assert_eq!(report.pane, ids[0]);
        assert_eq!(
            report.deadline_epoch, None,
            "a cancelled report carries no deadline"
        );
    }

    // The slot is gone, and the plugin was told before it went: `pane_closed`
    // carries the slot's identity, so it cannot be sent afterwards.
    assert!(
        logged_event(&f.log, |e| e["event"] == "pane_closed").is_some(),
        "the plugin must be told the slot it was holding is gone"
    );
    hub.stop();
}

#[test]
fn cancelling_a_pane_with_nothing_pending_is_harmless() {
    let f = fixture();
    let hub = TerminalHub::spawn(
        &f.cwd(),
        vec![watched("watched", "printf STILL-HERE; sleep 30")],
        vec![reporter(PLUGIN, &f)],
    );
    let session = hub.connect();
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&session, 1);

    // A live pane has no hold, and neither has an id that never existed.
    session.dispatch(ClientMessage::CancelRecovery { pane: ids[0] });
    session.dispatch(ClientMessage::CancelRecovery { pane: 4242 });

    assert!(
        !cancelled_within(&session, QUIET_WINDOW),
        "a cancel with nothing pending must announce nothing"
    );

    // And the worker is still serving: an unknown or inapplicable cancel is a
    // no-op, not an error that wedges the queue.
    session.dispatch(ClientMessage::Create {
        rows: ROWS,
        cols: COLS,
    });
    assert!(
        next_matching(&session, |frame| created_pane(frame)
            .is_some_and(|id| id != ids[0]))
        .is_some(),
        "the hub stopped serving after a cancel it had nothing to do with"
    );
    hub.stop();
}

/// Whether a `cancelled` report arrives inside `window`. Frames that are not
/// recovery reports are drained and ignored — a real pane is streaming output the
/// whole time.
fn cancelled_within(session: &TerminalSession, window: Duration) -> bool {
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        let Some(frame) = session.next_frame(Duration::from_millis(20)) else {
            continue;
        };
        if recovery(&frame).is_some_and(|r| r.state == RECOVERY_CANCELLED) {
            return true;
        }
    }
    false
}

#[test]
fn a_hold_that_runs_out_of_time_reports_the_pane_it_gave_up_on() {
    // The pane id is what the worker turns into the final `cancelled` broadcast,
    // so an expiry that retires the slot silently would leave every client
    // counting down to a moment that has passed.
    let f = fixture();
    let mut backend = PtyBackend::new(f.cwd());
    let mut plugins = Plugins::start(&f.cwd(), &[reporter(PLUGIN, &f)], &[opt_in()]);
    let pane = backend
        .open_pane(ROWS, COLS, Some(LONG_RUNNING))
        .expect("open a pane");
    let token = token_of(&backend, pane);
    assert!(plugins.adopt(pane, PLUGIN));
    let exited_at = Instant::now();
    backend.release_process(pane);
    plugins.hold_for_relaunch(
        pane,
        PaneSpot {
            index: 0,
            rows: ROWS,
            cols: COLS,
            title: None,
        },
        exited_at,
    );

    assert!(
        plugins
            .expire_pending(&mut backend, exited_at + PENDING_RELAUNCH_TTL / 2)
            .is_empty(),
        "a hold still inside its window must not be reported"
    );

    assert_eq!(
        plugins.expire_pending(&mut backend, exited_at + PENDING_RELAUNCH_TTL),
        vec![pane],
        "the expired pane must be named so its clients can be told"
    );
    assert_eq!(backend.pane_for_token(&token), None);
}
