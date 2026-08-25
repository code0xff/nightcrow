//! What a client attaching mid-recovery is told.
//!
//! A recovery report is broadcast once and the plugin does not repeat it, so
//! the hub caches each pane's last report and replays it on connect — a late
//! screen must show the same chip (and offer the same cancel) an early one
//! does. Unix-only for the reason `recovery.rs` is: the fake plugin is a shell.
#![cfg(unix)]

use super::plugin_rules::{LONG_RUNNING, PLUGIN};
use super::plugins::{fixture, logged_event};
use super::recovery::{
    QUIET_WINDOW, REPORTED_ATTEMPT, REPORTED_DEADLINE, REPORTED_DETAIL, REPORTED_STATE, Report,
    recovery, reporter, watched,
};
use super::{attach, collect_created, next_matching, spawn_hub};
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::ClientMessage;
use crate::session::terminal::hub_recovery::RECOVERY_CANCELLED;
use std::time::{Duration, Instant};

/// Whether any recovery report at all arrives inside `window`.
fn recovery_within(session: &TerminalSession, window: Duration) -> bool {
    let deadline = Instant::now() + window;
    while Instant::now() < deadline {
        let Some(frame) = session.next_frame(Duration::from_millis(20)) else {
            continue;
        };
        if recovery(&frame).is_some() {
            return true;
        }
    }
    false
}

#[test]
fn a_client_attaching_mid_recovery_is_replayed_the_standing_report() {
    let f = fixture();
    let hub = spawn_hub(
        &f.cwd(),
        vec![watched("watched", LONG_RUNNING)],
        vec![reporter(PLUGIN, &f)],
    );
    let early = attach(&hub);
    early.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&early, 1);
    // Only once the broadcast has left is the cache known to hold the report —
    // it is stored under the same lock the broadcast goes out under.
    next_matching(&early, |frame| recovery(frame).is_some())
        .expect("the report never reached the first client");

    let late = attach(&hub);

    let report = next_matching(&late, |frame| recovery(frame).is_some())
        .and_then(|frame| recovery(&frame))
        .expect("the late client was never told about the standing recovery");
    assert_eq!(
        report,
        Report {
            pane: ids[0],
            state: REPORTED_STATE.to_string(),
            detail: Some(REPORTED_DETAIL.to_string()),
            deadline_epoch: Some(REPORTED_DEADLINE),
            attempt: REPORTED_ATTEMPT,
        },
        "the replayed report must say what the broadcast said"
    );
    hub.stop();
}

#[test]
fn dropping_the_watcher_ends_its_standing_report_for_everyone() {
    // A reload that stops a plugin releases the live panes it was watching.
    // Nothing will ever update or cancel their reports again, so the hub ends
    // them itself: the screen showing the chip is told, and a late one is not
    // replayed a report whose cancel nothing would answer.
    let f = fixture();
    let hub = spawn_hub(
        &f.cwd(),
        vec![watched("watched", LONG_RUNNING)],
        vec![reporter(PLUGIN, &f)],
    );
    let early = attach(&hub);
    early.dispatch(ClientMessage::Start { sizes: Vec::new() });
    collect_created(&early, 1);
    next_matching(&early, |frame| recovery(frame).is_some())
        .expect("the report never reached the first client");

    assert!(hub.reload_plugins(Vec::new()), "the reload was not queued");

    next_matching(&early, |frame| {
        recovery(frame).is_some_and(|r| r.state == RECOVERY_CANCELLED)
    })
    .expect("the standing report was never ended");
    let late = attach(&hub);
    assert!(
        !recovery_within(&late, QUIET_WINDOW),
        "a report without a watcher must not be replayed"
    );
    hub.stop();
}

#[test]
fn stopping_the_hub_ends_its_standing_reports() {
    // The sockets outlive the hub: a client parked on one would keep an
    // orphan chip whose cancel nothing answers. `Exited` cannot stand in —
    // a report is meant to survive its pane's removal (the relaunch hold) —
    // so teardown ends each standing report itself.
    let f = fixture();
    let hub = spawn_hub(
        &f.cwd(),
        vec![watched("watched", LONG_RUNNING)],
        vec![reporter(PLUGIN, &f)],
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&session, 1);
    next_matching(&session, |frame| recovery(frame).is_some())
        .expect("the report never reached the client");

    hub.stop();

    let report = next_matching(&session, |frame| {
        recovery(frame).is_some_and(|r| r.state == RECOVERY_CANCELLED)
    })
    .and_then(|frame| recovery(&frame))
    .expect("the hub went down without ending the standing report");
    assert_eq!(report.pane, ids[0]);
}

#[test]
fn a_client_attaching_after_the_cancel_hears_nothing_of_it() {
    // The mirror rule's other half: `cancelled` removes the cache entry, so a
    // recovery that is over is not replayed as one still standing.
    let f = fixture();
    let hub = spawn_hub(
        &f.cwd(),
        vec![watched("watched", "printf gone; exit 0")],
        vec![reporter(PLUGIN, &f)],
    );
    let presser = attach(&hub);
    presser.dispatch(ClientMessage::Start { sizes: Vec::new() });
    let ids = collect_created(&presser, 1);
    next_matching(&presser, |frame| recovery(frame).is_some())
        .expect("the report never reached the cancelling client");
    assert!(
        logged_event(&f.log, |e| e["event"] == "pane_exited").is_some(),
        "the pane never exited into a hold"
    );

    presser.dispatch(ClientMessage::CancelRecovery { pane: ids[0] });
    next_matching(&presser, |frame| {
        recovery(frame).is_some_and(|r| r.state == RECOVERY_CANCELLED)
    })
    .expect("the cancel was never announced");

    let late = attach(&hub);

    assert!(
        !recovery_within(&late, QUIET_WINDOW),
        "a recovery that ended must not be replayed to a late client"
    );
    hub.stop();
}
