//! A pane driven the way the worker drives it, and the renderings its record is
//! compared against.
//!
//! Every test that needs to watch the worker's output path without a program behind
//! it shares this: chunks go through the hub's tracking emulator and record, the end
//! of each simulated tick runs the same paced pass, and a second emulator is fed the
//! same bytes as the client that was there all along. The clock is passed in rather
//! than read, so a window can be crossed without sleeping and a snapshot count is
//! exact instead of a race.

use super::{attach, spawn_hub};
use crate::backend::PaneId;
use crate::runtime::emulator::PaneEmulator;
use crate::session::terminal::TerminalHub;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(super) const PANE: PaneId = 11;
pub(super) const ROWS: u16 = 24;
pub(super) const COLS: u16 = 80;

/// The worker's own tick.
pub(super) const TICK: Duration = Duration::from_millis(8);

/// Nothing here races an assertion: every frame was queued before the client
/// attached, so this is a drain rather than a deadline.
const QUEUED: Duration = Duration::from_millis(200);

/// The paint that puts the pane on the alternate screen and gives it something
/// worth snapshotting.
pub(super) const ENTER: &[u8] = b"\x1b[?1049h\x1b[2J\x1b[1;1HHEADER";

/// A frame of the shape a program repaints on its own timer.
pub(super) fn frame(n: usize) -> Vec<u8> {
    format!("\x1b[2;1Hframe {n:04}\x1b[K").into_bytes()
}

/// A pane driven the way the worker drives one, without a process behind it.
pub(super) struct Desk {
    pub(super) hub: Arc<TerminalHub>,
    tracker: PaneModeTracker,
    /// What an attached client renders, fed the same bytes the pane produced.
    live: PaneEmulator,
    rows: u16,
    cols: u16,
    now: Instant,
    /// When the desk was built, so a test can say how far into the window it is.
    origin: Instant,
    /// Whether this tick's output left the pane for the end-of-tick pass.
    restless: bool,
    /// Whether the pass is paced. The ignored measurement runs the unthrottled
    /// worker as the "before" side of its comparison.
    pub(super) paced: bool,
    pub(super) snapshots: usize,
}

impl Desk {
    pub(super) fn new(dir: &tempfile::TempDir) -> Self {
        Self::sized(dir, ROWS, COLS)
    }

    pub(super) fn sized(dir: &tempfile::TempDir, rows: u16, cols: u16) -> Self {
        let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
        hub.register_pane(PANE, rows, cols, None, None);
        let now = Instant::now();
        Self {
            hub,
            tracker: PaneModeTracker::default(),
            live: PaneEmulator::new(rows, cols, 0),
            rows,
            cols,
            now,
            origin: now,
            restless: false,
            paced: true,
            snapshots: 0,
        }
    }

    /// One chunk of the pane's output, as the worker's event loop takes it. Reports
    /// whether the chunk left the pane for the end-of-tick pass.
    pub(super) fn output(&mut self, chunk: &[u8]) -> bool {
        self.live.process(chunk);
        let restless =
            self.hub
                .record_pane_output(PANE, chunk.to_vec(), &mut self.tracker, self.now);
        if restless {
            self.restless = true;
        }
        restless
    }

    /// The end of one tick: the snapshot pass, then the clock moves on. The pass
    /// runs even when this tick moved nothing, because a pane that owed bytes
    /// when it fell idle is due a trailing snapshot once its window passes.
    pub(super) fn tick(&mut self) {
        let restless = std::mem::take(&mut self.restless);
        let panes = if restless { vec![PANE] } else { Vec::new() };
        if self.paced {
            for (pane, screen) in self.tracker.settle_restless(&panes, self.now) {
                self.hub.store_screen(pane, screen);
                self.snapshots += 1;
            }
        } else if !panes.is_empty()
            && self.tracker.at_boundary(PANE)
            && let Some(screen) = self.tracker.snapshot(PANE)
        {
            self.hub.store_screen(PANE, screen);
            self.snapshots += 1;
        }
        self.now += TICK;
    }

    /// Resize the pane the way the worker's layout path does: the tracking grid
    /// and the client's, then a screen only if the recorded stream is at a clean
    /// boundary. A torn stream defers that screen to the next clean chunk.
    pub(super) fn resize(&mut self, rows: u16, cols: u16) {
        self.tracker.resize(PANE, rows, cols);
        self.live.resize(rows, cols);
        self.rows = rows;
        self.cols = cols;
        if self.tracker.at_boundary(PANE)
            && let Some(screen) = self.tracker.snapshot(PANE)
        {
            self.hub.store_screen(PANE, screen);
            self.tracker.mark_stored(PANE, self.now);
            self.snapshots += 1;
        }
    }

    /// The worker's clock-driven end of a synchronized update, and the client's:
    /// the record is told what the settled grid says, the pane joins the tick's
    /// pass, and the live emulator settles the same bytes the way a client would.
    pub(super) fn settle_sync(&mut self, now: Instant) {
        self.live.settle_sync();
        for (pane, observed) in self.tracker.settle_sync(now) {
            let alt = observed.modes.alt_screen;
            self.hub.store_settled(pane, observed);
            if alt {
                self.restless = true;
            }
        }
    }

    pub(super) fn elapsed(&self) -> Duration {
        self.now.saturating_duration_since(self.origin)
    }
}

/// One cell as a client would draw it, through the same surface the renderers use.
pub(super) fn drawn(emulator: &PaneEmulator) -> Vec<String> {
    let view = emulator.view();
    let (rows, cols) = view.size();
    let mut out = Vec::with_capacity(usize::from(rows) * usize::from(cols));
    for row in 0..rows {
        for col in 0..cols {
            let Some(cell) = view.cell(row, col) else {
                continue;
            };
            let mut contents = String::new();
            cell.append_contents(&mut contents);
            out.push(format!(
                "{contents}|{:?}/{:?}/{}{}{}{}{}",
                cell.fg(),
                cell.bg(),
                cell.bold() as u8,
                cell.italic() as u8,
                cell.underline() as u8,
                cell.inverse() as u8,
                cell.dim() as u8,
            ));
        }
    }
    out
}

/// The bytes a client attaching right now is replayed for the pane.
pub(super) fn replay_stream(desk: &Desk) -> Vec<u8> {
    let client = attach(&desk.hub);
    let mut out = Vec::new();
    while let Some(frame) = client.next_frame(QUEUED) {
        if let TerminalFrame::Output { pane, data } = frame
            && pane == PANE
        {
            out.extend(data);
        }
    }
    out
}

/// The screen a client attaching right now is replayed, and how many bytes that
/// took.
fn replay(desk: &Desk) -> (PaneEmulator, usize) {
    let stream = replay_stream(desk);
    let mut emulator = PaneEmulator::new(desk.rows, desk.cols, 0);
    emulator.process(&stream);
    (emulator, stream.len())
}

/// Require that the record the hub holds right now rebuilds the live screen cell
/// for cell, and return how many bytes that replay cost.
pub(super) fn assert_replay_matches_live(desk: &Desk) -> usize {
    let (replayed, bytes) = replay(desk);
    assert_eq!(
        drawn(&replayed),
        drawn(&desk.live),
        "a replay must rebuild the live screen"
    );
    assert_eq!(
        replayed.view().cursor_position(),
        desk.live.view().cursor_position(),
        "and leave the cursor in the same cell"
    );
    bytes
}

/// Require that a client replaying the record now, once its own clock has ended the
/// synchronized update the record carries, lands on the live screen.
pub(super) fn assert_replay_settles_to_live(desk: &Desk) -> usize {
    let (mut replayed, bytes) = replay(desk);
    replayed.settle_sync();
    assert_eq!(
        drawn(&replayed),
        drawn(&desk.live),
        "a replayed update must settle onto the live screen"
    );
    assert_eq!(
        replayed.view().cursor_position(),
        desk.live.view().cursor_position(),
        "and leave the cursor in the same cell"
    );
    bytes
}
