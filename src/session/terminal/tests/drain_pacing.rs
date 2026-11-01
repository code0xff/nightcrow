//! A burst must not outrun a client that keeps the polled worker's pace. A client
//! that falls a queue's depth behind is cut off rather than skipped, so a worker
//! woken per queued event has to keep the per-window drain ceiling the fixed tick
//! used to give it.

use super::pty_fixture::{fixture_hub, start_pane};
use crate::backend::PaneId;
use crate::session::terminal::frame::{ClientMessage, TerminalFrame};
use crate::session::terminal::{FrameWait, TerminalHub, TerminalSession};
use std::time::{Duration, Instant};

#[cfg(unix)]
const BURST: &str = "yes xxxxxxxxxxxxxxx | head -c 2097152; printf nightcrow-burst-done";
#[cfg(windows)]
const BURST: &str = concat!(
    "$out=[Console]::OpenStandardOutput(); ",
    "$bytes=[Text.Encoding]::ASCII.GetBytes(('x' * 2097152)); ",
    "$out.Write($bytes,0,$bytes.Length); $out.Flush(); ",
    "$marker=[Text.Encoding]::ASCII.GetBytes('nightcrow-burst-done'); ",
    "$out.Write($marker,0,$marker.Length); $out.Flush()"
);

const MARKER: &[u8] = b"nightcrow-burst-done";
const DEADLINE: Duration = Duration::from_secs(60);

/// Time spent per frame, well inside the polled ceiling of one 64-event budget
/// per 8 ms (one frame per 125 µs) even with a debug build's own per-frame cost.
/// Before the ceiling was kept across passes, a Linux client this slow was cut off
/// within the first 30 ms of the burst. macOS delivers PTY output in small chunks,
/// so a debug client there runs close to the ceiling, and a margin much smaller
/// than this one turns scheduling noise into a cut-off.
const PER_FRAME: Duration = Duration::from_micros(20);

enum Outcome {
    Marker,
    CutOff,
    TimedOut,
}

/// Read the burst as a consumer that spends [`PER_FRAME`] on each frame,
/// answering the cursor queries a PowerShell child waits on.
fn read_slowly(hub: &TerminalHub, session: &TerminalSession, pane: PaneId) -> Outcome {
    let started = Instant::now();
    let mut answered = 0;
    let mut tail: Vec<u8> = Vec::new();
    while started.elapsed() < DEADLINE {
        let data = match session.wait_frame(Duration::from_millis(100)) {
            FrameWait::Frame(TerminalFrame::Output { pane: p, data }) if p == pane => data,
            FrameWait::Closed => return Outcome::CutOff,
            // A cut-off client is removed from the hub, but its session can keep
            // waiting without ever seeing the channel close.
            FrameWait::Idle if hub.client_count() == 0 => return Outcome::CutOff,
            _ => continue,
        };
        let asked = data.windows(4).filter(|w| *w == b"\x1b[6n").count();
        while answered < asked {
            session.dispatch(ClientMessage::Input {
                pane,
                data: "\x1b[1;1R".to_string(),
            });
            answered += 1;
        }
        tail.extend_from_slice(&data);
        let keep = MARKER.len() * 2;
        if tail.len() > keep {
            tail.drain(..tail.len() - keep);
        }
        if tail.windows(MARKER.len()).any(|w| w == MARKER) {
            return Outcome::Marker;
        }
        let until = Instant::now() + PER_FRAME;
        while Instant::now() < until {
            std::hint::spin_loop();
        }
    }
    Outcome::TimedOut
}

#[test]
fn a_client_at_the_polled_pace_is_not_cut_off_by_a_burst() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = fixture_hub(&dir.path().to_string_lossy(), BURST);
    let (session, pane) = start_pane(&hub);

    let outcome = read_slowly(&hub, &session, pane);
    hub.stop();
    match outcome {
        Outcome::Marker => {}
        Outcome::CutOff => panic!("the client was cut off during the burst"),
        Outcome::TimedOut => panic!("the burst never finished reaching the client"),
    }
}
