use super::{attach, spawn_hub_with_auto_open};
use crate::runtime::emulator::PaneModes;
use crate::session::terminal::CLIENT_QUEUE_DEPTH;
use crate::session::terminal::frame::{ServerMessage, TerminalFrame};
use crate::session::terminal::hub_helpers::PaneState;
use crate::session::terminal::hub_modes::Observed;
use crate::session::terminal::hub_replay::{REPLAY_CHUNK_BYTES, ReplayQueue};
use crate::session::terminal::hub_replay_record::{Chunks, Sink};
use std::collections::VecDeque;
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
        Some(TerminalFrame::Output { pane: 7, data }) if &data[..] == b"LIVE"
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

/// The replay's frame cuts are part of the wire contract: a client applies the
/// concatenation, but one frame may never approach the 4 MiB socket ceiling, and
/// 1 MiB cuts are what keep the largest allowed replay inside the client queue
/// depth. Capture builds these frames, so it has to cut them where the old
/// `chunks` call did.
#[test]
fn a_replay_is_cut_into_the_same_one_mib_frames() {
    const PANE: u32 = 7;
    const ROWS: u16 = 24;
    const COLS: u16 = 80;
    let total = 2 * REPLAY_CHUNK_BYTES + 7;

    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    hub.state
        .lock()
        .expect("terminal state poisoned")
        .panes
        .push(PaneState {
            id: PANE,
            title: None,
            scrollback: std::iter::repeat_n(b'x', total).collect::<VecDeque<u8>>(),
            normal_screen: Vec::new(),
            covered: 0,
            screen: Vec::new(),
            since: VecDeque::new(),
            rows: ROWS,
            cols: COLS,
            modes: PaneModes::default(),
        });

    let session = attach(&hub);
    let mut sizes = Vec::new();
    let mut assembled = Vec::new();
    while let Some(frame) = session.next_frame(Duration::from_millis(300)) {
        if let TerminalFrame::Output { pane, data } = frame
            && pane == PANE
        {
            sizes.push(data.len());
            assembled.extend(data.iter().copied());
        }
    }
    hub.stop();

    assert!(
        sizes.len() == 4 && sizes[0] < REPLAY_CHUNK_BYTES,
        "the mode prelude leads in its own frame, got {sizes:?}"
    );
    assert_eq!(
        &sizes[1..],
        &[REPLAY_CHUNK_BYTES, REPLAY_CHUNK_BYTES, 7],
        "the record must be cut where the wire contract cuts it"
    );
    assert_eq!(assembled.len(), sizes.iter().sum::<usize>());
    assert!(
        assembled[sizes[0]..].iter().all(|&byte| byte == b'x'),
        "the record's bytes must arrive exactly once and in order"
    );
}

/// The cut points themselves, at the sizes where a chunk boundary is easiest to get
/// wrong: nothing at all, one byte, a byte short of a chunk, a chunk exactly, a byte
/// over, and two whole chunks. Capture fills these chunks in uneven writes, so the
/// boundary falls inside a write rather than the end of one.
#[test]
fn replay_chunks_are_cut_where_the_contract_cuts_them() {
    for total in [
        0,
        1,
        REPLAY_CHUNK_BYTES - 1,
        REPLAY_CHUNK_BYTES,
        REPLAY_CHUNK_BYTES + 1,
        2 * REPLAY_CHUNK_BYTES,
    ] {
        let bytes = vec![b'x'; total];
        let mut chunks = Chunks::new(total);
        for piece in bytes.chunks(7) {
            chunks.write(piece);
        }
        let chunks = chunks.finish();

        let sizes: Vec<usize> = chunks.iter().map(|chunk| chunk.len()).collect();
        let cuts: Vec<usize> = bytes.chunks(REPLAY_CHUNK_BYTES).map(<[u8]>::len).collect();
        assert_eq!(
            sizes, cuts,
            "a {total}-byte record must be cut where the contract cuts it"
        );
        assert!(
            chunks
                .iter()
                .flat_map(|chunk| chunk.iter().copied())
                .eq(bytes),
            "a {total}-byte record must arrive as its own bytes, once and in order"
        );
    }
}
