//! What a captured replay becomes on the wire: the queue is handed the very
//! allocations capture filled, in the order the contract names. This is the second
//! half of "copied once" — the first half is that capture composes each record
//! directly into those allocations.

use crate::runtime::emulator::PaneModes;
use crate::session::terminal::CLIENT_QUEUE_DEPTH;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_helpers::PaneState;
use crate::session::terminal::hub_replay::{PaneReplay, ReplayQueue, replay_pane};
use std::sync::Arc;
use std::sync::mpsc;

const PANE: u32 = 7;
const ROWS: u16 = 24;
const COLS: u16 = 80;

/// A pane with all three parts: a frozen normal buffer beneath, a program on the
/// alternate screen, and bytes recorded since that screen was taken.
fn an_alternate_screen_pane() -> PaneState {
    PaneState {
        id: PANE,
        title: Some("delivery".to_string()),
        scrollback: b"HISTORY\r\n".iter().copied().collect(),
        normal_screen: b"NORMAL SNAPSHOT".to_vec(),
        covered: 5,
        screen: b"\x1b[2JALT SCREEN".to_vec(),
        since: b"\x1b[3;1HALT TAIL".iter().copied().collect(),
        rows: ROWS,
        cols: COLS,
        modes: PaneModes {
            alt_screen: true,
            ..PaneModes::default()
        },
    }
}

fn addresses(chunks: &[Arc<[u8]>]) -> Vec<(usize, usize)> {
    chunks
        .iter()
        .map(|chunk| (chunk.as_ptr() as usize, chunk.len()))
        .collect()
}

/// A frame that carried a copy of a captured chunk would point somewhere else. The
/// chunks are alive for as long as `replay` is, so an address match is identity, not
/// a coincidence of reuse. The order is the contract's: the normal screen under the
/// program, the prelude, then the screen with its tail.
#[test]
fn the_queue_is_handed_the_captured_chunks_themselves() {
    let (tx, rx): (mpsc::SyncSender<TerminalFrame>, _) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH + 1);
    let replay = PaneReplay::capture(&an_alternate_screen_pane());
    let mut queue = ReplayQueue::new(&tx);
    assert!(
        replay_pane(&mut queue, &replay),
        "the whole replay must fit"
    );

    let mut delivered = Vec::new();
    while let Ok(frame) = rx.try_recv() {
        if let TerminalFrame::Output { pane, data } = frame {
            assert_eq!(pane, PANE, "every replay frame must name its pane");
            delivered.push((data.as_ptr() as usize, data.len()));
        }
    }

    let expected: Vec<(usize, usize)> = replay
        .parts()
        .iter()
        .flat_map(|part| addresses(part))
        .collect();
    assert!(
        expected.len() > 1,
        "the fixture must have more than one part for the order to mean anything"
    );
    assert_eq!(
        delivered, expected,
        "each frame must carry the captured allocation itself, in the contract's order"
    );
}

/// The control frames a client gets before the replay are not part of it: the pane is
/// announced, then its bytes arrive, and nothing is interleaved between the parts.
#[test]
fn the_announcement_precedes_every_byte_of_the_replay() {
    let (tx, rx): (mpsc::SyncSender<TerminalFrame>, _) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH + 1);
    let replay = PaneReplay::capture(&an_alternate_screen_pane());
    let mut queue = ReplayQueue::new(&tx);
    assert!(replay_pane(&mut queue, &replay));

    let frames: Vec<TerminalFrame> = rx.try_iter().collect();
    let Some(TerminalFrame::Control(created)) = frames.first() else {
        panic!("the replay must open by announcing its pane");
    };
    let created: serde_json::Value = serde_json::from_str(created).expect("control JSON");
    assert_eq!(created["type"], "created");
    assert_eq!(created["pane"].as_u64(), Some(u64::from(PANE)));
    assert_eq!(
        created["title"].as_str(),
        Some("delivery"),
        "a replayed pane keeps the title its program set"
    );
    assert!(
        frames[1..]
            .iter()
            .all(|frame| matches!(frame, TerminalFrame::Output { .. })),
        "nothing may be interleaved between the replay's bytes"
    );
}
