//! What stream-boundary tracking costs per byte, measured rather than assumed.
//!
//! Client emulators skip it (see
//! [`PaneEmulator::without_boundary_tracking`](super::PaneEmulator::without_boundary_tracking));
//! this measures what they no longer pay. It is not a behavioural test and does
//! not run with the suite. Rerun after changing the feed path with:
//!
//! ```text
//! cargo test --release measure_boundary_tracking_cost -- --ignored --nocapture
//! ```

use super::PaneEmulator;
use super::boundary::StreamBoundary;
use std::hint::black_box;
use std::time::{Duration, Instant};

const CHUNK_BYTES: usize = 4 * 1024;
const STREAM_BYTES: usize = 8 * 1024 * 1024;

const LINE: &[u8] =
    b"\x1b[38;5;208m2026-01-01T00:00:00Z\x1b[m INFO build step finished in 12ms\r\n";

fn stream() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(STREAM_BYTES + LINE.len());
    while bytes.len() < STREAM_BYTES {
        bytes.extend_from_slice(LINE);
    }
    bytes
}

fn feed_boundary(stream: &[u8]) -> Duration {
    let mut boundary = StreamBoundary::default();
    let start = Instant::now();
    for chunk in stream.chunks(CHUNK_BYTES) {
        boundary.feed(chunk);
    }
    let elapsed = start.elapsed();
    black_box(&boundary);
    elapsed
}

fn process_stream(mut emulator: PaneEmulator, stream: &[u8]) -> Duration {
    let start = Instant::now();
    for chunk in stream.chunks(CHUNK_BYTES) {
        emulator.process(chunk);
    }
    let elapsed = start.elapsed();
    black_box(&emulator);
    elapsed
}

#[test]
#[ignore]
fn measure_boundary_tracking_cost() {
    let stream = stream();
    let bytes = stream.len();
    let boundary = feed_boundary(&stream);
    let untracked = process_stream(PaneEmulator::without_boundary_tracking(24, 80, 0), &stream);
    let tracked = process_stream(PaneEmulator::new(24, 80, 0), &stream);

    eprintln!(
        "stream: {:.1} MiB in {CHUNK_BYTES}-byte reads",
        bytes as f64 / (1 << 20) as f64
    );
    eprintln!(
        "boundary feed alone: {boundary:?} ({:.2} ns/byte)",
        boundary.as_nanos() as f64 / bytes as f64
    );
    eprintln!(
        "emulator untracked: {untracked:?} ({:.1} MiB/s)",
        bytes as f64 / (1 << 20) as f64 / untracked.as_secs_f64()
    );
    eprintln!(
        "emulator tracked:   {tracked:?} ({:.1} MiB/s)",
        bytes as f64 / (1 << 20) as f64 / tracked.as_secs_f64()
    );
    eprintln!(
        "tracked/untracked: {:.3}x",
        tracked.as_secs_f64() / untracked.as_secs_f64()
    );
}
