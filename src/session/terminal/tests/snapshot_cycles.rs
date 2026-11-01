//! What a replay must reproduce when the record the hub keeps is not one continuous
//! stream: a pane leaving and re-entering the alternate screen, a chunk that splits a
//! sequence across ticks, a normal-screen record whose tail outgrows its ring, a switch
//! the clock applies instead of a byte, and a randomised stream of every such shape.
//!
//! Each is a place where the alternate record becomes a snapshot plus a tail, or where
//! the normal record stops being the whole ring — so the pace lengthening that tail is
//! only safe if exactly these paths still rebuild the live screen.

use super::snapshot_desk::{
    COLS, Desk, ENTER, PANE, ROWS, assert_replay_matches_live, assert_replay_settles_to_live,
    frame, replay_stream,
};
use crate::session::limits::MAX_TERMINAL_SCROLLBACK_BYTES;
use crate::session::terminal::hub_modes::PaneModeTracker;
use std::time::{Duration, Instant};

/// Past the 150 ms window the emulator arms on a synchronized update opening, so a
/// test can reach an expired one without sleeping through it.
const PAST_SYNC_WINDOW: Duration = Duration::from_millis(200);

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

/// Bytes owed on the alternate screen belong to that record. Once the program
/// leaves it, the pass must not take the normal grid as if it were the owed
/// alternate screen, however long the old window had left to run.
#[test]
fn leaving_the_alternate_screen_drops_the_bytes_owed_on_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = super::spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.register_pane(PANE, ROWS, COLS, None, None);
    let now = Instant::now();
    let mut tracker = PaneModeTracker::default();

    assert!(
        !hub.record_pane_output(PANE, ENTER.to_vec(), &mut tracker, now),
        "the entry chunk takes its own screen"
    );
    assert!(
        hub.record_pane_output(PANE, b"\x1b[2;1Hmore".to_vec(), &mut tracker, now),
        "painting on top of it leaves bytes owed"
    );
    assert!(
        !hub.record_pane_output(PANE, b"\x1b[?1049lback".to_vec(), &mut tracker, now),
        "the leave chunk is recorded against the normal screen"
    );
    assert!(
        tracker
            .settle_restless(&[], now + Duration::from_millis(250))
            .is_empty(),
        "and nothing owed on the alternate screen survives the switch"
    );
    hub.stop();
}

/// A program killed inside a synchronized update that was carrying the switch to
/// the alternate screen. The clock applies it, and the record was told about the
/// switch only afterwards — its bytes went into the normal record, which is what a
/// replay carries them from. The pace may still defer the alternate screen itself,
/// so the client ends the update on its own clock and must land on the live screen.
#[test]
fn a_settled_switch_to_the_alternate_screen_replays_once_the_client_settles_too() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut desk = Desk::new(&dir);

    desk.output(ENTER);
    desk.tick();
    desk.output(b"\x1b[?1049lbye");
    desk.tick();
    // Opened and never closed: the switch and the paint are held, and the record
    // files them against the normal screen.
    desk.output(b"\x1b[?2026h\x1b[?1049h\x1b[2J\x1b[3;1HPAINTED");
    desk.tick();

    desk.settle_sync(Instant::now() + PAST_SYNC_WINDOW);
    // Inside the window, so the pass leaves the missing alternate screen for later.
    desk.tick();
    assert_replay_settles_to_live(&desk);

    // Once the window passes the record carries that screen itself, and every later
    // replay still settles where the live pane is.
    for n in 0..63 {
        desk.output(&frame(n));
        desk.tick();
    }
    assert_replay_settles_to_live(&desk);
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
    let windows = elapsed_ms / 250;
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
