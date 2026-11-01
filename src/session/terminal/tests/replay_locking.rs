//! The replay is framed with the hub unlocked, and live bytes can only follow it.

use super::{attach, spawn_hub_with_auto_open};
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::session::limits;
use crate::session::terminal::ConcurrencyTestPoint;
use crate::session::terminal::TerminalHub;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_helpers::PaneState;
use crate::session::terminal::hub_modes::Observed;
use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
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

    let (framing, reached) = mpsc::channel();
    let (resume, wait) = mpsc::channel();
    let wait = std::sync::Mutex::new(wait);
    hub.set_concurrency_test_hook(move |point| {
        if point == ConcurrencyTestPoint::ReplayFraming {
            framing.send(()).unwrap();
            wait.lock().expect("resume gate poisoned").recv().unwrap();
        }
    });

    let connecting = {
        let hub = Arc::clone(&hub);
        thread::spawn(move || attach(&hub))
    };
    reached
        .recv_timeout(Duration::from_secs(5))
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
    hub.record_and_broadcast(0, b"LIVE".to_vec(), observed(), None);
    resume.send(()).unwrap();

    let session = connecting.join().expect("the connect thread panicked");
    let mut complete = false;
    let mut live = 0;
    while let Some(frame) = session.next_frame(QUIET) {
        match control_type(&frame).as_deref() {
            Some("replay_complete") => complete = true,
            Some(_) => {}
            None => {
                if let TerminalFrame::Output { pane: 0, data } = &frame
                    && &data[..] == b"LIVE"
                {
                    assert!(complete, "live output overtook the replay");
                    live += 1;
                }
            }
        }
    }
    assert!(complete, "the replay never completed");
    assert_eq!(live, 1, "the held frame must arrive exactly once");
    hub.stop();
}

/// Output recorded after the snapshot but before the completion marker belongs to
/// the live stream: after the replay, and exactly once.
#[test]
fn output_recorded_right_after_the_snapshot_follows_the_replay() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    install_full_ring(&hub, 0);

    let session = attach(&hub);
    hub.record_and_broadcast(0, b"LIVE".to_vec(), observed(), None);

    let mut complete = false;
    let mut live = 0;
    while let Some(frame) = session.next_frame(QUIET) {
        match control_type(&frame).as_deref() {
            Some("replay_complete") => complete = true,
            Some(_) => {}
            None => {
                if let TerminalFrame::Output { pane: 0, data } = &frame
                    && &data[..] == b"LIVE"
                {
                    assert!(complete, "live output overtook the replay");
                    live += 1;
                }
            }
        }
    }
    assert!(complete, "the replay never completed");
    assert_eq!(live, 1, "one broadcast must not be replayed or duplicated");
    hub.stop();
}
