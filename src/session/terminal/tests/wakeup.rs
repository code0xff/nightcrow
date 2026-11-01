//! What wakes a parked worker.
//!
//! Each test parks the worker first — the pass count has stopped moving — so the
//! frame it waits for can only arrive because a producer signalled, not because a
//! pass happened to be due.

use super::{
    PROMPT, attach, created_pane, exited_pane, next_matching, pty_fixture::fixture_hub,
    pty_fixture::settle_startup, pty_fixture::start_pane, resized_size, spawn_hub_with_auto_open,
    wait_until_parked,
};
use crate::backend::PaneId;
use crate::session::terminal::TerminalHub;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{ClientMessage, TerminalFrame};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// What the shell is asked to print. Both `cmd.exe` and a POSIX shell answer
/// `echo`, so the fixture needs no platform branch.
const MARKER: &[u8] = b"nightcrow-wake";

/// A fixture that answers one line from its stdin and then exits, so what the
/// client sees after the hub parks is only the reply to what the test typed.
#[cfg(unix)]
const ECHO_LINE: &str = "read line; printf 'nightcrow-%s' \"$line\"";
#[cfg(windows)]
const ECHO_LINE: &str = "[Console]::Write('nightcrow-'+[Console]::ReadLine())";

/// Far past any wake test's deadline: with the cap out of reach, only a producer's
/// signal can bring the worker back in time, so a missing one fails the test
/// rather than being rescued by the cap.
const HIGH_CAP: Duration = Duration::from_secs(30);

/// A hub with nothing to do: no startup terminals and no clients.
fn idle_hub(dir: &tempfile::TempDir) -> Arc<TerminalHub> {
    raise_cap(spawn_hub_with_auto_open(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        false,
    ))
}

fn raise_cap(hub: Arc<TerminalHub>) -> Arc<TerminalHub> {
    hub.set_wait_cap(HIGH_CAP);
    hub
}

/// Ask for one terminal and wait for its id.
fn open_pane(session: &TerminalSession) -> PaneId {
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let created = next_matching(session, |f| created_pane(f).is_some()).expect("no created frame");
    created_pane(&created).expect("a created frame names its pane")
}

#[test]
fn stop_wakes_a_parked_worker() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = idle_hub(&dir);
    wait_until_parked(&hub);

    let started = Instant::now();
    hub.stop();
    while !hub.worker_finished() && started.elapsed() < PROMPT {
        thread::sleep(Duration::from_millis(5));
    }
    let took = started.elapsed();
    assert!(
        hub.worker_finished(),
        "the worker was still parked {took:?} after `stop`"
    );
}

#[test]
fn a_queued_command_wakes_a_parked_worker() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = raise_cap(spawn_hub_with_auto_open(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        true,
    ));
    let session = attach(&hub);
    wait_until_parked(&hub);

    let started = Instant::now();
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let created = next_matching(&session, |f| created_pane(f).is_some());
    let took = started.elapsed();
    assert!(created.is_some(), "the create command was not taken");
    assert!(took < PROMPT, "the command waited {took:?} for a pass");
    hub.stop();
}

#[test]
fn a_queued_close_wakes_a_parked_worker() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = raise_cap(spawn_hub_with_auto_open(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        true,
    ));
    let session = attach(&hub);
    let pane = open_pane(&session);
    wait_until_parked(&hub);

    let started = Instant::now();
    session.dispatch(ClientMessage::Close { pane });
    let exited = next_matching(&session, |f| exited_pane(f) == Some(pane));
    let took = started.elapsed();
    assert!(exited.is_some(), "the pane was never closed");
    assert!(took < PROMPT, "the close waited {took:?} for a pass");
    hub.stop();
}

#[test]
fn a_queued_resize_wakes_a_parked_worker() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = raise_cap(spawn_hub_with_auto_open(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        true,
    ));
    let session = attach(&hub);
    let pane = open_pane(&session);
    wait_until_parked(&hub);

    let started = Instant::now();
    session.dispatch(ClientMessage::Resize {
        pane,
        rows: 20,
        cols: 60,
    });
    let resized = next_matching(&session, |f| resized_size(f) == Some((20, 60)));
    let took = started.elapsed();
    assert!(resized.is_some(), "the resize was never applied");
    assert!(took < PROMPT, "the resize waited {took:?} for a pass");
    hub.stop();
}

#[test]
fn output_from_a_shell_wakes_a_parked_worker() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = raise_cap(fixture_hub(&dir.path().to_string_lossy(), ECHO_LINE));
    let (session, pane) = start_pane(&hub);
    // The child asks where the cursor is before it reads; a client answers, and
    // only then is the hub's worker left with nothing to do.
    settle_startup(&session, pane, Duration::from_millis(200));
    wait_until_parked(&hub);

    let started = Instant::now();
    session.dispatch(ClientMessage::Input {
        pane,
        data: "wake\r".to_string(),
    });
    let output = next_matching(&session, |f| {
        matches!(f, TerminalFrame::Output { pane: p, data }
            if *p == pane && data.windows(MARKER.len()).any(|window| window == MARKER))
    });
    let took = started.elapsed();
    assert!(output.is_some(), "the fixture's reply never arrived");
    assert!(took < PROMPT, "output waited {took:?} for a pass");
    hub.stop();
}

/// Passes a hub makes while idle, with and without the wake signal. Not a
/// behavioural test and not run with the suite; rerun it when the wait changes.
///
/// ```text
/// cargo test --release measure_idle_wakes -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn measure_idle_wakes_over_two_seconds() {
    const WINDOW: Duration = Duration::from_secs(2);
    let mut measured = Vec::new();
    for legacy_polling in [true, false] {
        let dir = tempfile::TempDir::new().unwrap();
        let hub = idle_hub(&dir);
        hub.set_legacy_polling(legacy_polling);
        // Let the first pass happen before the window opens.
        thread::sleep(Duration::from_millis(100));
        let before = hub.worker_passes();
        thread::sleep(WINDOW);
        let passes = hub.worker_passes() - before;
        measured.push((legacy_polling, passes));
        hub.stop();
    }
    for (legacy, passes) in &measured {
        let label = if *legacy {
            "fixed 8 ms tick"
        } else {
            "wake signal"
        };
        println!("{label}: {passes} passes in {WINDOW:?}");
    }
    let (legacy, woken) = (measured[0].1, measured[1].1);
    assert!(
        legacy >= 100,
        "the fixed tick must reproduce a tick per wait, got {legacy}"
    );
    assert!(
        woken * 10 < legacy,
        "the wake signal must cost far fewer passes than the tick: {woken} vs {legacy}"
    );
}
