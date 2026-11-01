//! Replay composition at this hub's caps. A benchmark, not a test: ignored so the
//! suite does not pay for it. Run with `--ignored --nocapture`.

use super::super::hub_helpers::PaneState;
use super::{PaneReplay, ReplayQueue, replay_pane};
use crate::runtime::emulator::PaneModes;
use crate::session::limits;
use std::collections::VecDeque;
use std::sync::mpsc;
use std::time::Instant;

const ROUNDS: usize = 5;

/// The largest record this hub keeps for one pane: a full ring at the cap plus a
/// snapshot and the uncovered tail it is allowed to exceed the cap by.
fn pane(id: u32) -> PaneState {
    let ring = 3 * limits::MAX_TERMINAL_SCROLLBACK_BYTES;
    PaneState {
        id,
        title: Some("bench".into()),
        scrollback: std::iter::repeat_n(b'x', ring).collect(),
        normal_screen: vec![b'y'; limits::MAX_TERMINAL_SCROLLBACK_BYTES / 4],
        covered: limits::MAX_TERMINAL_SCROLLBACK_BYTES,
        screen: Vec::new(),
        since: VecDeque::new(),
        rows: limits::MAX_PANE_ROWS,
        cols: limits::MAX_PANE_COLS,
        modes: PaneModes::default(),
    }
}

/// The build this replaced: one push per byte of the ring.
fn byte_by_byte(pane: &PaneState) -> Vec<u8> {
    let mut data = Vec::with_capacity(pane.scrollback.len() + pane.normal_screen.len());
    data.extend(pane.scrollback.iter().copied().take(pane.covered));
    data.extend_from_slice(&pane.normal_screen);
    data.extend(pane.scrollback.iter().copied().skip(pane.covered));
    data
}

fn median(mut samples: Vec<std::time::Duration>) -> std::time::Duration {
    samples.sort();
    samples[samples.len() / 2]
}

#[test]
#[ignore = "benchmark"]
fn replay_build_at_full_caps() {
    let panes: Vec<PaneState> = (0..limits::MAX_PTYS_PER_REPO as u32).map(pane).collect();
    let (tx, rx) = mpsc::sync_channel(4096);

    let mut legacy = Vec::with_capacity(ROUNDS);
    let mut build = Vec::with_capacity(ROUNDS);
    let mut framed = Vec::with_capacity(ROUNDS);
    for _ in 0..ROUNDS {
        let start = Instant::now();
        let old: Vec<Vec<u8>> = panes.iter().map(byte_by_byte).collect();
        legacy.push(start.elapsed());

        let start = Instant::now();
        let replays: Vec<PaneReplay> = panes.iter().map(PaneReplay::capture).collect();
        build.push(start.elapsed());
        assert!(
            old.iter()
                .zip(&replays)
                .all(|(old, replay)| *old == replay.record)
        );

        let start = Instant::now();
        let mut queue = ReplayQueue::new(&tx);
        for replay in &replays {
            assert!(replay_pane(&mut queue, replay));
        }
        framed.push(start.elapsed());
        while rx.try_recv().is_ok() {}
    }

    println!(
        "{} panes, {} bytes each: byte-by-byte build {:?}, snapshot build {:?}, framing {:?}",
        panes.len(),
        panes[0].scrollback.len() + panes[0].normal_screen.len(),
        median(legacy),
        median(build),
        median(framed),
    );
}
