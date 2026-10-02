use super::{attach, spawn_hub_with_auto_open};
use crate::runtime::emulator::PaneModes;
use crate::session::terminal::CLIENT_QUEUE_DEPTH;
use crate::session::terminal::frame::{ServerMessage, TerminalFrame};
use crate::session::terminal::hub_modes::Observed;
use crate::session::terminal::hub_replay::ReplayQueue;
use std::sync::mpsc;
use std::time::Duration;

fn control_type(frame: &TerminalFrame) -> Option<String> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    serde_json::from_str::<serde_json::Value>(json).ok()?["type"]
        .as_str()
        .map(str::to_string)
}

#[test]
fn empty_replay_marker_precedes_the_initial_size_verdict() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    let session = attach(&hub);

    assert_eq!(
        control_type(&session.next_frame(Duration::ZERO).unwrap()).as_deref(),
        Some("hello")
    );
    assert_eq!(
        control_type(&session.next_frame(Duration::ZERO).unwrap()).as_deref(),
        Some("replay_complete")
    );
    assert_eq!(
        control_type(&session.next_frame(Duration::ZERO).unwrap()).as_deref(),
        Some("size_owner")
    );
    hub.record_and_broadcast(
        7,
        b"LIVE".to_vec(),
        Observed {
            modes: PaneModes::default(),
            title: None,
            alt_changed: false,
        },
        None,
    );
    assert!(matches!(
        session.next_frame(Duration::ZERO),
        Some(TerminalFrame::Output { pane: 7, data }) if data == b"LIVE"
    ));
    hub.stop();
}

#[test]
fn replayed_pane_is_announced_before_completion_and_completion_precedes_live_frames() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    hub.register_pane(7, 24, 80, None, None);

    let session = attach(&hub);
    let mut saw_created = false;
    loop {
        let frame = session.next_frame(Duration::ZERO).expect("replay frame");
        match control_type(&frame).as_deref() {
            Some("created") => saw_created = true,
            Some("replay_complete") => break,
            Some("size_owner") => panic!("size ownership arrived before replay completion"),
            _ => {}
        }
    }
    assert!(saw_created, "the replay marker must follow its pane record");
    assert_eq!(
        control_type(&session.next_frame(Duration::ZERO).unwrap()).as_deref(),
        Some("size_owner")
    );
    hub.stop();
}

#[test]
fn a_stopped_hub_still_marks_its_empty_replay_complete() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    hub.stop();

    let session = attach(&hub);

    assert_eq!(
        control_type(&session.next_frame(Duration::ZERO).unwrap()).as_deref(),
        Some("hello")
    );
    assert_eq!(
        control_type(&session.next_frame(Duration::ZERO).unwrap()).as_deref(),
        Some("replay_complete")
    );
}

#[test]
fn replay_frame_budget_leaves_one_slot_for_completion() {
    let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH + 1);
    let mut replay = ReplayQueue::new(&tx);
    for _ in 0..CLIENT_QUEUE_DEPTH {
        assert!(replay.send(TerminalFrame::Control("{}".into())));
    }
    assert!(!replay.send(TerminalFrame::Control("{}".into())));
    tx.try_send(TerminalFrame::Control(
        serde_json::to_string(&ServerMessage::ReplayComplete).unwrap(),
    ))
    .expect("the reserved slot accepts completion");
    assert_eq!(rx.try_iter().count(), CLIENT_QUEUE_DEPTH + 1);
}
