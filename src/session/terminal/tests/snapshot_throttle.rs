//! What a busy alternate-screen pane costs to keep replayable, and what a client
//! connecting between two of its snapshots is given.
//!
//! The chunks and the ticks are the worker's own: each chunk goes through its
//! tracking emulator and record, and the end of each simulated tick runs the same
//! paced pass. The clock is passed in rather than read, so a window can be crossed
//! without sleeping and a snapshot count is exact instead of a race.

use super::{attach, spawn_hub};
use crate::backend::PaneId;
use crate::runtime::emulator::PaneEmulator;
use crate::session::limits::MAX_TERMINAL_SCROLLBACK_BYTES;
use crate::session::terminal::TerminalHub;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::sync::Arc;
use std::time::{Duration, Instant};

const PANE: PaneId = 11;
const ROWS: u16 = 24;
const COLS: u16 = 80;

/// The worker's own tick.
const TICK: Duration = Duration::from_millis(8);

/// Nothing here races an assertion: every frame was queued before the client
/// attached, so this is a drain rather than a deadline.
const QUEUED: Duration = Duration::from_millis(200);

/// The paint that puts the pane on the alternate screen and gives it something
/// worth snapshotting.
const ENTER: &[u8] = b"\x1b[?1049h\x1b[2J\x1b[1;1HHEADER";

/// A frame of the shape a program repaints on its own timer.
fn frame(n: usize) -> Vec<u8> {
    format!("\x1b[2;1Hframe {n:04}\x1b[K").into_bytes()
}

/// A pane driven the way the worker drives one, without a process behind it.
struct Desk {
    hub: Arc<TerminalHub>,
    tracker: PaneModeTracker,
    /// What an attached client renders, fed the same bytes the pane produced.
    live: PaneEmulator,
    now: Instant,
    /// When the desk was built, so a test can say how far into the window it is.
    origin: Instant,
    /// Whether this tick's output left the pane for the end-of-tick pass.
    restless: bool,
    /// Whether the pass is paced. The ignored measurement runs the unthrottled
    /// worker as the "before" side of its comparison.
    paced: bool,
    snapshots: usize,
}

impl Desk {
    fn new(dir: &tempfile::TempDir) -> Self {
        let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
        hub.register_pane(PANE, ROWS, COLS, None, None);
        let now = Instant::now();
        Self {
            hub,
            tracker: PaneModeTracker::default(),
            live: PaneEmulator::new(ROWS, COLS, 0),
            now,
            origin: now,
            restless: false,
            paced: true,
            snapshots: 0,
        }
    }

    /// One chunk of the pane's output, as the worker's event loop takes it.
    fn output(&mut self, chunk: &[u8]) {
        self.live.process(chunk);
        if self
            .hub
            .record_pane_output(PANE, chunk.to_vec(), &mut self.tracker)
        {
            self.restless = true;
        }
    }

    /// The end of one tick: the snapshot pass, then the clock moves on.
    fn tick(&mut self) {
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

    fn elapsed(&self) -> Duration {
        self.now.saturating_duration_since(self.origin)
    }
}

/// One cell as a client would draw it, through the same surface the renderers use.
fn drawn(emulator: &PaneEmulator) -> Vec<String> {
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

/// The screen a client attaching right now is replayed, and how many bytes that
/// took.
fn replay(desk: &Desk) -> (PaneEmulator, usize) {
    let client = attach(&desk.hub);
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    let mut bytes = 0;
    while let Some(frame) = client.next_frame(QUEUED) {
        if let TerminalFrame::Output { pane, data } = frame
            && pane == PANE
        {
            bytes += data.len();
            emulator.process(&data);
        }
    }
    (emulator, bytes)
}

/// Require that the record the hub holds right now rebuilds the live screen cell
/// for cell, and return how many bytes that replay cost.
fn assert_replay_matches_live(desk: &Desk) -> usize {
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

/// The window is wider than a repaint: a program that redraws several times per
/// tick costs one serialization per window instead of one per clean boundary, and
/// what it sent in between is still replayed on top of that screen.
#[test]
fn many_small_writes_inside_the_window_cost_one_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();
    let baseline = desk.snapshots;

    // 64 ticks of 8 ms: the last is 504 ms after the baseline, just past the
    // window. Every tick carried output.
    for n in 0..64 {
        desk.output(&frame(n));
        desk.tick();
    }
    assert_eq!(
        desk.snapshots - baseline,
        1,
        "64 ticks of output inside one window must cost one snapshot, not one each"
    );
    assert!(
        desk.elapsed() >= Duration::from_millis(500),
        "and the fixture must have reached the end of the window"
    );
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}

/// Waiting out the window is not worth making the replay hundreds of kilobytes
/// long: past the owed cap the screen is taken as soon as the stream ends clean.
#[test]
fn output_past_the_owed_cap_is_snapshotted_inside_the_window() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();
    let baseline = desk.snapshots;

    // 1 KiB per write, a tick every eight: 80 KiB in 80 ms, well inside the
    // 500 ms window, so only the owed bytes can have asked for the snapshot.
    let chunk = vec![b'x'; 1024];
    for n in 0..80 {
        desk.output(&chunk);
        if n % 8 == 7 {
            desk.tick();
        }
    }
    desk.tick();

    assert!(
        desk.snapshots > baseline,
        "over the owed cap a snapshot must not wait for the window"
    );
    assert!(
        desk.elapsed() < Duration::from_millis(500),
        "and this fixture must still be inside it, was {:?}",
        desk.elapsed()
    );
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}

/// The record is still bounded, and throttling does not change that: past the ring
/// cap a fresh screen supersedes what was owed, so a replay of a megabyte of output
/// stays close to the cap rather than growing with everything the program wrote.
#[test]
fn a_record_past_the_crowded_cap_keeps_its_replay_bounded() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();

    let chunk = vec![b'y'; 4096];
    for n in 0..256 {
        desk.output(&chunk);
        if n % 16 == 15 {
            desk.tick();
        }
    }
    desk.tick();

    let replay_bytes = assert_replay_matches_live(&desk);
    assert!(
        replay_bytes < MAX_TERMINAL_SCROLLBACK_BYTES + 64 * 1024,
        "a megabyte of output must not replay as {replay_bytes} bytes"
    );
    desk.hub.stop();
}

/// Snapshots a busy pane costs per second, with and without the pace. Not a
/// behavioural test and not run with the suite; kept because it is the reason the
/// pace exists, and rerun after changing the pass:
///
/// ```text
/// cargo test --release measure_snapshots_per_second -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn measure_snapshots_per_second_for_a_busy_pane() {
    // Two seconds of the worker's tick.
    let ticks = usize::try_from(Duration::from_secs(2).as_millis() / TICK.as_millis()).unwrap();
    let mut measured = Vec::new();
    for paced in [false, true] {
        let dir = tempfile::TempDir::new().unwrap();
        let mut desk = Desk::new(&dir);
        desk.paced = paced;
        desk.output(ENTER);
        desk.tick();
        let started = Instant::now();
        for n in 0..ticks {
            desk.output(&frame(n));
            desk.tick();
        }
        let elapsed = started.elapsed();
        let replay_bytes = assert_replay_matches_live(&desk);
        measured.push((paced, desk.snapshots, elapsed, replay_bytes));
        desk.hub.stop();
    }
    for (paced, snapshots, elapsed, replay_bytes) in measured {
        let label = if paced { "paced" } else { "every tick" };
        eprintln!(
            "{label}: {snapshots} snapshots over {ticks} ticks ({:.0}/s), \
             {elapsed:?} of work, replay {replay_bytes} bytes",
            snapshots as f64 / 2.0
        );
    }
}
