//! The whole path, from a real child on a real PTY to a client that was not there: a
//! program paints the alternate screen for longer than the pace's window, so what a
//! later client is replayed is a paced snapshot plus the tail, and both clients must
//! end up looking at the same screen.
//!
//! Windows only: the fixture is PowerShell writing to a ConPTY. On Unix the same path
//! is covered by [`screen_replay`](super::screen_replay) with a shell fixture. The
//! child waits on a cursor-position query that a real client's emulator answers, so
//! this reader answers it too — frames alone would leave the pane stuck before its
//! first paint.
#![cfg(windows)]

use super::snapshot_desk::drawn;
use super::{SHELL_TEST_DEADLINE, attach, created_pane, next_matching};
use crate::backend::PaneId;
use crate::config::{ShellConfig, StartupCommand};
use crate::runtime::emulator::PaneEmulator;
use crate::session::terminal::TerminalHub;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{ClientMessage, PaneSize, TerminalFrame};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ROWS: u16 = 24;
const COLS: u16 = 80;
const MARKER: &str = "DONEPROBE";
const CURSOR_QUERY: &[u8] = b"\x1b[6n";
const CURSOR_REPLY: &str = "\x1b[1;1R";

/// A program that owns the alternate screen and repaints it for longer than one pace
/// window, then leaves a marker and stays alive so a second client can attach.
const PAINT: &str = concat!(
    "$e=[char]27; [Console]::Write($e+'[?1049h'); ",
    "for($i=0;$i -lt 30;$i++){[Console]::Write($e+'[2;1Hframe '+$i); ",
    "Start-Sleep -Milliseconds 20}; ",
    "[Console]::Write($e+'[3;1HDONEPROBE'); Start-Sleep -Seconds 10",
);

/// A hub whose configured startup terminal is the painter, with PowerShell as the
/// pane shell so the script reaches it as one argument rather than through `cmd`'s
/// quoting.
fn paint_hub(dir: &tempfile::TempDir) -> Arc<TerminalHub> {
    TerminalHub::spawn(
        &dir.path().to_string_lossy(),
        vec![StartupCommand {
            name: None,
            command: PAINT.to_string(),
            plugin: None,
        }],
        Vec::new(),
        false,
        ShellConfig {
            program: Some("powershell".to_string()),
            command_args: vec!["-NoProfile".to_string(), "-Command".to_string()],
        },
        Default::default(),
    )
}

/// Everything `session` is sent for `pane` up to `marker`, with the child's cursor
/// queries answered as a client emulator would. Reports how many output frames it
/// arrived in, so a paint the PTY coalesced into one read cannot pass for a paced one.
fn read_until(session: &TerminalSession, pane: PaneId, marker: &str) -> (Vec<u8>, usize) {
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    let mut out = Vec::new();
    let mut answered = 0;
    let mut frames = 0;
    while Instant::now() < deadline {
        if let Some(frame) = session.next_frame(Duration::from_millis(100))
            && let TerminalFrame::Output { pane: p, data } = frame
            && p == pane
        {
            frames += 1;
            out.extend_from_slice(&data);
            let asked = out
                .windows(CURSOR_QUERY.len())
                .filter(|window| *window == CURSOR_QUERY)
                .count();
            while answered < asked {
                session.dispatch(ClientMessage::Input {
                    pane,
                    data: CURSOR_REPLY.to_string(),
                });
                answered += 1;
            }
        }
        if out
            .windows(marker.len())
            .any(|window| window == marker.as_bytes())
        {
            return (out, frames);
        }
    }
    panic!(
        "the pane never painted {marker:?}, got: {:?}",
        String::from_utf8_lossy(&out)
    )
}

fn rendered(stream: &[u8]) -> PaneEmulator {
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    emulator.process(stream);
    emulator
}

#[test]
fn a_real_alternate_screen_program_replays_exactly_after_a_paced_window() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = paint_hub(&dir);
    let original = attach(&hub);
    original.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize {
            rows: ROWS,
            cols: COLS,
        }],
    });
    let created = next_matching(&original, |frame| created_pane(frame).is_some()).expect("no pane");
    let pane = created_pane(&created).expect("no pane id");

    // More than one pace window of paint, then a marker.
    let (live, frames) = read_until(&original, pane, MARKER);
    let reattached = attach(&hub);
    let (replay, _) = read_until(&reattached, pane, MARKER);
    hub.stop();

    assert!(
        frames > 1,
        "the paint must reach the worker in more than one read, or it never crossed a tick"
    );
    assert!(
        live.windows(8).any(|window| window == b"frame 29"),
        "the fixture must have painted its whole window, got: {:?}",
        String::from_utf8_lossy(&live)
    );
    assert!(
        live.windows(8).any(|window| window == b"\x1b[?1049h"),
        "the fixture must have taken the alternate screen, got: {:?}",
        String::from_utf8_lossy(&live)
    );
    assert!(
        replay.windows(7).any(|window| window == b"\x1b[m\x1b[2J"),
        "the replayed record must carry a screen, not only the bytes that followed one, \
         got: {:?}",
        String::from_utf8_lossy(&replay)
    );
    let from_live = rendered(&live);
    let from_replay = rendered(&replay);
    assert_eq!(
        drawn(&from_replay),
        drawn(&from_live),
        "a client replayed the paced record must see the screen the original does"
    );
    assert_eq!(
        from_replay.view().cursor_position(),
        from_live.view().cursor_position(),
        "and leave the cursor in the same cell"
    );
}
