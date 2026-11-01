//! What a replay must reproduce when a pane's mode changes while its screen is
//! being paced: leaving and re-entering the alternate screen, and a chunk that
//! splits a sequence across ticks.
//!
//! Both are the places where the record the hub keeps is not one continuous
//! stream — the alternate screen becomes a snapshot plus a tail, and the tail may
//! be replayed raw — so the pace lengthening that tail is only safe if exactly
//! these paths still rebuild the live screen.

use super::snapshot_throttle::{Desk, ENTER, assert_replay_matches_live, frame, replay_stream};
use crate::session::limits::MAX_TERMINAL_SCROLLBACK_BYTES;

/// The normal screen under the program is recorded, spent, frozen and spent again
/// as the program comes and goes. Every step of that must rebuild the live screen,
/// with the pace holding some of the alternate screens back.
#[test]
fn a_pane_that_leaves_and_returns_to_the_alternate_screen_still_replays_exactly() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);

    desk.output(b"plain shell output");
    desk.tick();
    assert_replay_matches_live(&desk);

    desk.output(b"\x1b[?1049h\x1b[2J\x1b[1;1HFULL SCREEN");
    desk.tick();
    assert_replay_matches_live(&desk);

    // Inside the window, so these frames stay in the tail on top of the screen.
    for n in 0..8 {
        desk.output(&frame(n));
    }
    desk.tick();
    assert_replay_matches_live(&desk);

    desk.output(b"\x1b[?1049lback to the shell");
    desk.tick();
    assert_replay_matches_live(&desk);

    // Back on again: the entry chunk's screen is taken by the chunk itself.
    desk.output(b"\x1b[?1049h\x1b[2Jagain");
    desk.tick();
    assert_replay_matches_live(&desk);

    for n in 8..16 {
        desk.output(&frame(n));
    }
    desk.tick();
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}

/// A chunk that ends inside a sequence is filed into the tail raw, and the tick
/// that closes it is the one that may take a screen — unless the pace is still
/// inside its window, which leaves the raw bytes in place a while longer. Either
/// way the client replaying them lands on the live screen.
#[test]
fn a_torn_chunk_is_replayed_raw_until_a_tick_takes_the_screen() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);

    desk.output(ENTER);
    desk.tick();
    assert_replay_matches_live(&desk);

    // A cursor position cut in half: the pass must not splice a screen here.
    desk.output(b"\x1b[2;1HTORN\x1b[");
    desk.tick();
    assert_replay_matches_live(&desk);

    // The stream closes, but the pace has not: the tail still carries both
    // chunks, and a client applying them lands where the pane already is.
    desk.output(b"1;9HCLOSED");
    desk.tick();
    assert_replay_matches_live(&desk);

    // Cross the window, so the screen is finally taken over that tail.
    for n in 0..63 {
        desk.output(&frame(n));
        desk.tick();
    }
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}

/// One chunk of a stream the fixtures would never pick: a repaint, a long run of
/// filler, or a lone escape introducer that leaves the next chunk inside a
/// sequence. Fixed seed, so a failure reproduces.
fn next_chunk(state: &mut u64, n: usize) -> Vec<u8> {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    match *state % 16 {
        0..=9 => format!("\x1b[2;1Hframe {n:06}\x1b[K").into_bytes(),
        10..=13 => vec![b'.'; 64 + (*state as usize % 8192)],
        14 => b"\x1b[".to_vec(),
        _ => b"\x1b[3;1H\x1b[K".to_vec(),
    }
}

/// Chunk sizes and tick spacing the hand-written fixtures do not reach, with the
/// replay compared against the live screen every few ticks. Also the pace's own
/// promise: a snapshot after the first needs either a whole window or another owed
/// cap of output, so the count cannot creep back toward one per tick.
#[test]
fn any_chunking_keeps_the_replay_exact_and_the_snapshot_rate_bounded() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    desk.output(ENTER);
    desk.tick();
    let mut written = ENTER.len();
    let mut state = 0x9e37_79b9_7f4a_7c15u64;

    for tick in 0..250 {
        for n in 0..=tick % 3 {
            let chunk = next_chunk(&mut state, written + n);
            written += chunk.len();
            desk.output(&chunk);
        }
        desk.tick();
        if tick % 50 == 0 {
            assert_replay_matches_live(&desk);
        }
    }
    assert_replay_matches_live(&desk);

    let elapsed_ms = desk.elapsed().as_millis() as u64;
    let windows = elapsed_ms / 500;
    let caps = written as u64 / (64 * 1024);
    assert!(
        desk.snapshots as u64 <= windows + caps + 2,
        "{} paced snapshots in {elapsed_ms} ms over {written} bytes: more than {windows} \
         windows and {caps} owed caps allow",
        desk.snapshots
    );
    assert!(
        written > MAX_TERMINAL_SCROLLBACK_BYTES,
        "the stream must be long enough to cross the crowded cap"
    );
    desk.hub.stop();
}

/// The other branch of the worker's output path, which the extraction moved too: a
/// normal-screen pane records its ring, never joins the alternate-screen pass, and
/// only a tail grown past the cap asks for a snapshot — which the replay must then
/// carry instead of the ring it supersedes.
#[test]
fn a_crowded_normal_record_still_takes_its_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);
    let chunk = b"a line of ordinary output\r\n".repeat(8);
    let chunks = MAX_TERMINAL_SCROLLBACK_BYTES / chunk.len() + 64;

    for n in 0..chunks {
        assert!(
            !desk.output(&chunk),
            "a normal-screen pane is never left for the alternate-screen pass"
        );
        if n % 4 == 3 {
            desk.tick();
        }
    }
    desk.tick();

    let stream = replay_stream(&desk);
    assert!(
        stream.windows(4).any(|window| window == b"\x1b[2J"),
        "the replay must carry the absolute repaint a crowded ring asks for"
    );
    assert_replay_matches_live(&desk);
    desk.hub.stop();
}
