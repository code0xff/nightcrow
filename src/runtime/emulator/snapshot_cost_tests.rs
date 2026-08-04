//! What a snapshot costs to take, measured rather than assumed.
//!
//! This is not a behavioural test and does not run with the suite. It is kept
//! because it is the reason [`snapshot`](super::snapshot) appends into one string
//! instead of building them: at a `String` per cell, a densely coloured large
//! screen took 14 ms to serialize — longer than the worker tick that takes it.
//! Rerun it after changing the emit path:
//!
//! ```text
//! cargo test --release measure_snapshot_cost -- --ignored --nocapture
//! ```
//!
//! Measured on the author's machine after that change, per snapshot:
//!
//! | screen | ordinary content | every cell a different colour |
//! |---|---|---|
//! | 24x80 | 5 µs, 2 KiB | 135 µs, 35 KiB |
//! | 50x200 | 34 µs, 10 KiB | 401 µs, 194 KiB |
//! | 130x500 | 185 µs, 64 KiB | 2.8 ms, 1.3 MiB |
//!
//! The left column is what a pane actually costs on the tick that snapshots it.
//! The right column is the ceiling, and what reaches it is a truecolour image
//! renderer filling a large pane.

use super::PaneEmulator;
use std::fmt::Write as _;

#[test]
#[ignore]
fn measure_snapshot_cost() {
    /// Every cell written and every cell a different colour: the worst a program
    /// can do, which a truecolour image renderer really does.
    fn dense(rows: u16, cols: u16) -> String {
        let mut paint = String::from("\x1b[?1049h");
        for row in 0..rows {
            let _ = write!(paint, "\x1b[{};1H", row + 1);
            for col in 0..cols {
                let _ = write!(
                    paint,
                    "\x1b[38;2;{};{};{}mX",
                    row % 255,
                    col % 255,
                    (row + col) % 255
                );
            }
        }
        paint
    }

    /// What an agent or a build log looks like: text, a colour every few words,
    /// and the right-hand side of most rows empty.
    fn ordinary(rows: u16, cols: u16) -> String {
        let mut paint = String::from("\x1b[?1049h");
        for row in 0..rows {
            let _ = write!(paint, "\x1b[{};1H", row + 1);
            for word in 0..(cols / 12) {
                let _ = write!(paint, "\x1b[3{}mword \x1b[m", word % 8);
            }
        }
        paint
    }

    for (rows, cols) in [(24u16, 80u16), (50, 200), (130, 500)] {
        for (what, paint) in [
            ("dense", dense(rows, cols)),
            ("ordinary", ordinary(rows, cols)),
        ] {
            let mut emulator = PaneEmulator::new(rows, cols, 0);
            emulator.process(paint.as_bytes());
            let runs = 200;
            let start = std::time::Instant::now();
            let mut bytes = 0;
            for _ in 0..runs {
                bytes += emulator.screen_snapshot().len();
            }
            eprintln!(
                "{rows}x{cols} {what}: {:?}/snapshot, {} bytes",
                start.elapsed() / runs,
                bytes / runs as usize
            );
        }
    }
}
