//! What a replay must reproduce when a pane's mode changes while its screen is
//! being paced: leaving and re-entering the alternate screen, and a chunk that
//! splits a sequence across ticks.
//!
//! Both are the places where the record the hub keeps is not one continuous
//! stream — the alternate screen becomes a snapshot plus a tail, and the tail may
//! be replayed raw — so the pace lengthening that tail is only safe if exactly
//! these paths still rebuild the live screen.

use super::snapshot_throttle::{Desk, ENTER, assert_replay_matches_live, frame};

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
