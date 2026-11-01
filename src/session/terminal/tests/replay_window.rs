//! The window between taking a replay snapshot and its completion marker: what
//! the hub holds back, what it lets through, and what it does to a client that
//! cannot take the replay at all.

use super::{
    HOOK_DEADLINE, attach, created_pane, exited_pane, hello_panes, pause_at_framing,
    spawn_hub_with_auto_open,
};
use crate::config::ShellConfig;
use crate::runtime::emulator::PaneModes;
use crate::session::size_owner::{SizeOwnership, ViewerId};
use crate::session::terminal::frame::{ServerMessage, TerminalFrame};
use crate::session::terminal::hub_modes::Observed;
use crate::session::terminal::{CLIENT_QUEUE_DEPTH, ConcurrencyTestPoint, TerminalHub};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const ROWS: u16 = 24;
const COLS: u16 = 80;
const QUIET: Duration = Duration::from_millis(300);

fn observed() -> Observed {
    Observed {
        modes: PaneModes::default(),
        title: None,
        alt_changed: false,
    }
}

fn control_type(frame: &TerminalFrame) -> Option<String> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    serde_json::from_str::<serde_json::Value>(json).ok()?["type"]
        .as_str()
        .map(str::to_string)
}

fn drain(session: &crate::session::terminal::TerminalSession) -> Vec<TerminalFrame> {
    let mut frames = Vec::new();
    while let Some(frame) = session.next_frame(QUIET) {
        frames.push(frame);
    }
    frames
}

fn position_of(frames: &[TerminalFrame], want: impl Fn(&TerminalFrame) -> bool) -> Option<usize> {
    frames.iter().position(want)
}

/// A connection that unwinds between registering its client and returning the
/// session must take the registration with it. Otherwise the client sits in the
/// broadcast list behind a replay gate nothing will ever open: every later frame
/// is held until it overflows, and no session exists to release anything.
#[test]
fn a_connect_that_unwinds_leaves_no_client_behind() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    hub.set_concurrency_test_hook(|point| {
        if point == ConcurrencyTestPoint::ReplayFraming {
            panic!("a connect that dies mid-replay");
        }
    });

    let connecting = {
        let hub = Arc::clone(&hub);
        thread::spawn(move || {
            let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| attach(&hub)));
            assert!(died.is_err(), "the connection was meant to unwind");
        })
    };
    connecting
        .join()
        .expect("the panic must not escape its thread");

    assert_eq!(
        hub.client_count(),
        0,
        "an unwound connect must unregister the client it registered"
    );
    hub.stop();
}

/// A pane opened and closed while the replay is framed still has to be announced
/// to the client — after the replay, since it happened after the snapshot — and
/// the greeting must keep promising only what the snapshot held.
#[test]
fn a_pane_opened_and_closed_during_the_window_follows_the_replay() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    hub.register_pane(7, ROWS, COLS, None, None);

    let (reached, resume) = pause_at_framing(&hub);
    let connecting = {
        let hub = Arc::clone(&hub);
        thread::spawn(move || attach(&hub))
    };
    reached
        .recv_timeout(HOOK_DEADLINE)
        .expect("connect never reached the framing stage");
    hub.register_pane(9, ROWS, COLS, None, None);
    hub.remove_pane_and_announce(9);
    resume.send(()).unwrap();

    let session = connecting.join().expect("the connect thread panicked");
    let frames = drain(&session);
    hub.stop();

    let promised = frames
        .iter()
        .find_map(hello_panes)
        .expect("the greeting must arrive");
    assert_eq!(
        promised, 1,
        "the greeting must count the snapshot, not what moved during the window"
    );
    let complete = position_of(&frames, |f| {
        control_type(f).as_deref() == Some("replay_complete")
    })
    .expect("the replay never completed");
    let opened = position_of(&frames, |f| created_pane(f) == Some(9))
        .expect("the pane opened during the window was never announced");
    let closed = position_of(&frames, |f| exited_pane(f) == Some(9))
        .expect("the pane closed during the window was never announced");
    assert!(
        complete < opened,
        "a pane opened during the window must be announced after the replay"
    );
    assert!(
        complete < closed && opened < closed,
        "the close must follow both the replay and the pane it closes"
    );
}

/// An addressed send for the connecting client is held with the gate, so the
/// client learns about the replay before anything that assumes it.
#[test]
fn an_addressed_send_during_the_window_waits_for_the_replay() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    hub.register_pane(7, ROWS, COLS, None, None);

    let (reached, resume) = pause_at_framing(&hub);
    let connecting = {
        let hub = Arc::clone(&hub);
        thread::spawn(move || attach(&hub))
    };
    reached
        .recv_timeout(HOOK_DEADLINE)
        .expect("connect never reached the framing stage");
    let client = hub
        .state
        .lock()
        .expect("terminal state poisoned")
        .clients
        .first()
        .expect("the connecting client is registered")
        .id;
    hub.send_to(client, &ServerMessage::Pending { count: 3 });
    resume.send(()).unwrap();

    let session = connecting.join().expect("the connect thread panicked");
    let frames = drain(&session);
    hub.stop();

    let complete = position_of(&frames, |f| {
        control_type(f).as_deref() == Some("replay_complete")
    })
    .expect("the replay never completed");
    let pending = position_of(&frames, |f| control_type(f).as_deref() == Some("pending"))
        .expect("the addressed send was never delivered");
    assert!(
        complete < pending,
        "an addressed send must not overtake the replay it was held behind"
    );
    assert_eq!(
        frames
            .iter()
            .filter(|f| control_type(f).as_deref() == Some("pending"))
            .count(),
        1,
        "the held send must be delivered exactly once"
    );
}

/// More frames than a client's queue holds, broadcast while its replay is not
/// queued yet, end the connection — and its size registration still goes when the
/// session that would have unregistered it is dropped.
#[test]
fn a_gate_that_overflows_during_the_window_cuts_the_client_off() {
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let ownership: Arc<SizeOwnership> = Default::default();
    let hub = TerminalHub::spawn(
        &cwd,
        Vec::new(),
        Vec::new(),
        false,
        ShellConfig::default(),
        Arc::clone(&ownership),
    );
    let viewer = ViewerId::Browser("overrun".to_string());

    let (reached, resume) = pause_at_framing(&hub);
    let connecting = {
        let hub = Arc::clone(&hub);
        let viewer = viewer.clone();
        thread::spawn(move || hub.connect(viewer, true, None))
    };
    reached
        .recv_timeout(HOOK_DEADLINE)
        .expect("connect never reached the framing stage");
    for _ in 0..=CLIENT_QUEUE_DEPTH {
        hub.record_and_broadcast(0, b"x".to_vec(), observed(), None);
    }
    assert_eq!(
        hub.client_count(),
        0,
        "a client whose held frames overflow its queue depth must be cut off"
    );
    resume.send(()).unwrap();

    let session = connecting.join().expect("the connect thread panicked");
    let connection = session.connection;
    assert!(
        ownership.owns(connection),
        "the connection registered before it was cut off"
    );
    drop(session);
    assert!(
        !ownership.owns(connection),
        "dropping an evicted client's session must still release the sizing"
    );
    hub.stop();
}
