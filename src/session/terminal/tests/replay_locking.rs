//! The replay is framed with the hub unlocked, and live bytes can only follow it.

use super::{HOOK_DEADLINE, attach, pause_at_framing, spawn_hub_with_auto_open};
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::session::limits;
use crate::session::terminal::TerminalHub;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_helpers::PaneState;
use crate::session::terminal::hub_modes::Observed;
use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const ROWS: u16 = 24;
const COLS: u16 = 80;
const QUIET: Duration = Duration::from_millis(300);
const LIVE: &[u8] = b"LIVE";

fn observed() -> Observed {
    Observed {
        modes: PaneModes::default(),
        title: None,
        alt_changed: false,
    }
}

/// Install a pane whose record is the largest this hub keeps: a full ring with
/// no covered mark, so every byte of it is replayed.
fn install_full_ring(hub: &Arc<TerminalHub>, id: PaneId) {
    let mut state = hub.state.lock().expect("terminal state poisoned");
    state.panes.push(PaneState {
        id,
        title: None,
        scrollback: std::iter::repeat_n(b'x', 3 * limits::MAX_TERMINAL_SCROLLBACK_BYTES).collect(),
        normal_screen: Vec::new(),
        covered: 0,
        screen: Vec::new(),
        since: VecDeque::new(),
        rows: ROWS,
        cols: COLS,
        modes: PaneModes::default(),
    });
}

fn control_type(frame: &TerminalFrame) -> Option<String> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    serde_json::from_str::<serde_json::Value>(json).ok()?["type"]
        .as_str()
        .map(str::to_string)
}

fn carries(frame: &TerminalFrame, needle: &[u8]) -> bool {
    matches!(
        frame,
        TerminalFrame::Output { data, .. }
            if data.windows(needle.len()).any(|window| window == needle)
    )
}

/// Drain `session` to quiescence and return how many frames after
/// `replay_complete` carry the live bytes.
///
/// The contract is exactly-once: a replay is a window of the same stream the live
/// frames continue, so a byte is either replayed or broadcast — never both, and
/// never before the marker that says the replay is done.
fn drain_exactly_once(session: &TerminalSession) -> usize {
    let mut complete = false;
    let mut before = 0;
    let mut after = 0;
    while let Some(frame) = session.next_frame(QUIET) {
        match control_type(&frame).as_deref() {
            Some("replay_complete") => complete = true,
            Some(_) => {}
            None if carries(&frame, LIVE) => {
                if complete {
                    after += 1;
                } else {
                    before += 1;
                }
            }
            None => {}
        }
    }
    assert!(complete, "the replay never completed");
    assert_eq!(
        before, 0,
        "a replay must not carry the bytes that were broadcast after its snapshot"
    );
    after
}

/// The lock is released before any of a large replay is framed, so another pane's
/// output is recorded and broadcast while that attach is still in progress.
#[test]
fn live_output_is_broadcast_while_a_large_replay_is_framed() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    for id in 0..limits::MAX_PTYS_PER_REPO as PaneId {
        install_full_ring(&hub, id);
    }

    let (reached, resume) = pause_at_framing(&hub);
    let connecting = {
        let hub = Arc::clone(&hub);
        thread::spawn(move || attach(&hub))
    };
    reached
        .recv_timeout(HOOK_DEADLINE)
        .expect("connect never reached the framing stage");

    // The snapshot is taken, the replay is not framed or queued yet, and the
    // worker can take the lock.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while hub.state.try_lock().is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "the replay must be framed with the hub state unlocked"
        );
        thread::sleep(Duration::from_millis(5));
    }
    hub.record_and_broadcast(0, LIVE.to_vec(), observed(), None);
    resume.send(()).unwrap();

    let session = connecting.join().expect("the connect thread panicked");
    let live = drain_exactly_once(&session);
    assert_eq!(live, 1, "the held frame must arrive exactly once");
    hub.stop();
}

/// Output recorded after the snapshot but before the completion marker is held
/// with the gate and released after it: the replay's own window does not repeat
/// it, and the live stream does not lose it.
#[test]
fn output_recorded_right_after_the_snapshot_follows_the_replay() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    install_full_ring(&hub, 0);

    let (reached, resume) = pause_at_framing(&hub);
    let connecting = {
        let hub = Arc::clone(&hub);
        thread::spawn(move || attach(&hub))
    };
    reached
        .recv_timeout(HOOK_DEADLINE)
        .expect("connect never reached the framing stage");
    hub.record_and_broadcast(0, LIVE.to_vec(), observed(), None);
    resume.send(()).unwrap();

    let session = connecting.join().expect("the connect thread panicked");
    let live = drain_exactly_once(&session);
    assert_eq!(live, 1, "one broadcast must not be replayed or duplicated");
    hub.stop();
}
