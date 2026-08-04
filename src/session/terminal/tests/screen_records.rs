//! The two records a pane's screen can live in, and the invariant the whole
//! reattach path rests on: **what a connecting client is replayed is exactly what
//! the clients already attached have been sent** — never a byte short, never a
//! byte twice.
//!
//! Driven by handing the hub chunks and screens directly instead of through a
//! program. The window this is about is a few milliseconds wide (a client
//! connecting between the broadcast of a chunk and the tick that snapshots it), so
//! a real pane could only reach it by luck; what decides it is which record each
//! chunk is filed against, and that is what these set up on purpose.

use super::attach;
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::TerminalFrame;
use crate::session::terminal::hub_helpers::REPLAY_CHUNK_BYTES;
use crate::session::terminal::hub_modes::Observed;
use std::time::Duration;

const PANE: PaneId = 7;
const ROWS: u16 = 24;
const COLS: u16 = 80;

/// Nothing here waits on a program: every frame is queued before the client
/// attaches, so this is a drain rather than a deadline.
const QUEUED: Duration = Duration::from_millis(200);

fn observed(alt_screen: bool, alt_changed: bool) -> Observed {
    Observed {
        modes: PaneModes {
            alt_screen,
            ..PaneModes::default()
        },
        title: None,
        alt_changed,
    }
}

/// Every output frame this pane's replay arrived in, in order.
fn replay_frames(session: &TerminalSession) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    while let Some(frame) = session.next_frame(QUEUED) {
        if let TerminalFrame::Output { pane, data } = frame
            && pane == PANE
        {
            frames.push(data);
        }
    }
    frames
}

fn replay_text(session: &TerminalSession) -> String {
    let joined: Vec<u8> = replay_frames(session).concat();
    String::from_utf8_lossy(&joined).to_string()
}

/// A hub with one pane registered but no process behind it — the records are the
/// subject, and a PTY would only decide when they were written.
fn hub_with_a_pane(dir: &tempfile::TempDir) -> std::sync::Arc<super::super::TerminalHub> {
    let hub = super::spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.register_pane(PANE, ROWS, COLS, None, None);
    hub
}

fn assert_in_order(text: &str, first: &str, then: &str) {
    let (a, b) = (text.find(first), text.find(then));
    assert!(
        a.is_some() && b.is_some() && a < b,
        "{first:?} must be replayed before {then:?}, got: {text:?}"
    );
}

#[test]
fn a_screen_is_replayed_with_everything_broadcast_since_it_was_taken() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    // The chunk that entered the alternate screen is handed over together with the
    // screen it produced.
    hub.record_and_broadcast(
        PANE,
        b"ENTERED".to_vec(),
        observed(true, true),
        Some(b"SCREEN".to_vec()),
    );
    // What arrives after it is owed on top of that screen until the next snapshot.
    hub.record_and_broadcast(PANE, b"AFTER".to_vec(), observed(true, false), None);

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    assert_in_order(&text, "SCREEN", "AFTER");
    // The snapshot already accounts for the chunk that came with it. Replaying
    // that chunk as well would put its bytes on the screen twice.
    assert!(
        !text.contains("ENTERED"),
        "a chunk the screen accounts for must not be replayed too, got: {text:?}"
    );
}

#[test]
fn a_fresh_screen_supersedes_what_was_owed_on_the_one_before_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    hub.record_and_broadcast(
        PANE,
        b"ENTERED".to_vec(),
        observed(true, true),
        Some(b"STALE".to_vec()),
    );
    hub.record_and_broadcast(PANE, b"AFTER".to_vec(), observed(true, false), None);
    // What the worker does at the end of a tick, once its emulator has caught up
    // with everything above.
    hub.store_screen(PANE, b"CURRENT".to_vec());

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    assert!(text.contains("CURRENT"), "got: {text:?}");
    for spent in ["STALE", "AFTER"] {
        assert!(
            !text.contains(spent),
            "{spent:?} is accounted for by the newer screen, got: {text:?}"
        );
    }
}

#[test]
fn leaving_the_alternate_screen_puts_the_byte_ring_back_in_charge() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    hub.record_and_broadcast(
        PANE,
        b"ENTERED".to_vec(),
        observed(true, true),
        Some(b"SCREEN".to_vec()),
    );
    hub.record_and_broadcast(PANE, b"BACK".to_vec(), observed(false, true), None);

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    assert!(
        text.contains("BACK"),
        "a normal-screen pane is replayed its ring, got: {text:?}"
    );
    assert!(
        !text.contains("SCREEN"),
        "the screen kept for the alternate buffer is spent, got: {text:?}"
    );
}

/// While a program owns the alternate screen the ring is left as it was, because
/// that is the screen the program will be returned to. A client that attaches
/// during the program has to be given it, or quitting the program leaves that
/// client looking at nothing.
#[test]
fn the_normal_screen_under_an_alternate_screen_program_is_replayed_too() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    hub.record_and_broadcast(PANE, b"BEFORE".to_vec(), observed(false, false), None);
    hub.record_and_broadcast(
        PANE,
        b"ENTERED".to_vec(),
        observed(true, true),
        Some(b"SCREEN".to_vec()),
    );

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    // Ahead of the prelude that switches away from the normal buffer, so it lands
    // on the one the program will come back to.
    assert_in_order(&text, "BEFORE", "\x1b[?1049h");
    assert_in_order(&text, "\x1b[?1049h", "SCREEN");
}

/// A screen is not capped, only the frames carrying it are. A large pane covered
/// in per-cell colour runs to megabytes, and sent whole it was refused by the
/// daemon socket -- which ended the attach connection, and ended it again on every
/// reconnect, because the same screen was replayed each time.
#[test]
fn a_screen_too_large_for_one_frame_is_replayed_in_several() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    // Deliberately not a multiple of the chunk size, so the last frame is a
    // partial one.
    let screen: Vec<u8> = (0..REPLAY_CHUNK_BYTES * 2 + 12_345)
        .map(|i| b'a' + (i % 26) as u8)
        .collect();
    hub.record_and_broadcast(
        PANE,
        b"ENTERED".to_vec(),
        observed(true, true),
        Some(screen.clone()),
    );

    let client = attach(&hub);
    let frames = replay_frames(&client);
    hub.stop();

    for (index, frame) in frames.iter().enumerate() {
        assert!(
            frame.len() <= REPLAY_CHUNK_BYTES,
            "frame {index} carries {} bytes, over the cap",
            frame.len()
        );
    }
    assert!(
        frames.len() >= 3,
        "a screen of {} bytes must not arrive in {} frames",
        screen.len(),
        frames.len()
    );
    // Where the frames fall does not matter to a client -- it concatenates them
    // into a parser that spans writes -- so what has to hold is that concatenating
    // them gives back exactly the screen, after the prelude that precedes it.
    let joined: Vec<u8> = frames.concat();
    assert!(
        joined.ends_with(&screen),
        "the frames must reassemble into the screen"
    );
}
