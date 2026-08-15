//! The normal screen's snapshot record: `ring[..covered]` + snapshot +
//! `ring[covered..]`, and the crowded signal that keeps it gapless.
//!
//! The scenario this guards: a program that repaints in place — Claude Code's
//! prompt box, any spinner — rotates the byte ring without scrolling, so after a
//! long idle the bytes that painted the top of the screen were evicted and a
//! reattaching client saw only the bottom. The snapshot is what still holds that
//! screen; these tests pin where it lands in the replay and what may be evicted
//! around it. Driven like [`screen_records`](super::screen_records): chunks and
//! screens handed to the hub directly, because which record each byte is filed
//! against is the subject.

use super::attach;
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::session::limits::MAX_TERMINAL_SCROLLBACK_BYTES;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::TerminalFrame;
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

fn replay_text(session: &TerminalSession) -> String {
    let mut joined = Vec::new();
    while let Some(frame) = session.next_frame(QUEUED) {
        if let TerminalFrame::Output { pane, data } = frame
            && pane == PANE
        {
            joined.extend(data);
        }
    }
    String::from_utf8_lossy(&joined).to_string()
}

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
fn a_normal_snapshot_is_replayed_between_covered_history_and_the_tail() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    hub.record_and_broadcast(PANE, b"HISTORY".to_vec(), observed(false, false), None);
    // What the worker does when the record asks (or a resize reflows the grid):
    // a screen that accounts for everything recorded so far.
    hub.store_normal_screen(PANE, b"SNAPSHOT".to_vec());
    hub.record_and_broadcast(PANE, b"TAIL".to_vec(), observed(false, false), None);

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    // History first — its scrolled-off lines are what the client can scroll back
    // to — then the screen, then what was recorded on top of it. Contiguous and
    // at the very end: nothing dropped, nothing doubled, nothing in between.
    assert!(
        text.ends_with("HISTORYSNAPSHOTTAIL"),
        "the record must be spliced exactly, got: {text:?}"
    );
}

#[test]
fn a_ring_whose_uncovered_tail_outgrows_the_cap_asks_for_a_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    let calm = hub.record_and_broadcast(PANE, b"SMALL".to_vec(), observed(false, false), None);
    assert!(
        calm <= MAX_TERMINAL_SCROLLBACK_BYTES,
        "a ring under the cap wants nothing"
    );

    // Nothing is covered yet, so nothing may be evicted: past the cap the record
    // must ask for the snapshot that starts covering.
    let flood = vec![b'x'; MAX_TERMINAL_SCROLLBACK_BYTES];
    let owed = hub.record_and_broadcast(PANE, flood, observed(false, false), None);
    assert!(
        owed > MAX_TERMINAL_SCROLLBACK_BYTES,
        "an uncovered ring past the cap must ask for a snapshot"
    );

    // With the snapshot stored, the history it covers is what eviction spends —
    // the next chunk fits by evicting it, and nothing more is asked for.
    hub.store_normal_screen(PANE, b"SNAPSHOT".to_vec());
    let after = hub.record_and_broadcast(PANE, b"TAIL".to_vec(), observed(false, false), None);
    assert!(
        after <= MAX_TERMINAL_SCROLLBACK_BYTES,
        "covered history absorbs eviction"
    );

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    // The eviction spent the oldest covered bytes, not the screen or the tail.
    assert!(
        text.ends_with("SNAPSHOTTAIL"),
        "the screen and the tail must survive eviction intact, got the end: {:?}",
        &text[text.len().saturating_sub(40)..]
    );
    assert!(
        !text.contains("SMALL"),
        "the oldest covered history is what eviction must have spent"
    );
}

#[test]
fn the_normal_screen_under_an_alternate_screen_program_carries_its_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    hub.record_and_broadcast(PANE, b"HISTORY".to_vec(), observed(false, false), None);
    hub.store_normal_screen(PANE, b"SNAPSHOT".to_vec());
    hub.record_and_broadcast(PANE, b"TAIL".to_vec(), observed(false, false), None);
    hub.record_and_broadcast(
        PANE,
        b"ENTERED".to_vec(),
        observed(true, true),
        Some(b"ALTSCREEN".to_vec()),
    );

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    // The whole normal record, spliced exactly, ahead of the switch to the
    // buffer the program is drawing on.
    assert!(
        text.contains("HISTORYSNAPSHOTTAIL"),
        "the normal record must arrive whole and in order, got: {text:?}"
    );
    assert_in_order(&text, "TAIL", "\x1b[?1049h");
    assert_in_order(&text, "\x1b[?1049h", "ALTSCREEN");
}

#[test]
fn returning_to_the_normal_screen_keeps_its_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = hub_with_a_pane(&dir);

    hub.record_and_broadcast(PANE, b"HISTORY".to_vec(), observed(false, false), None);
    hub.store_normal_screen(PANE, b"SNAPSHOT".to_vec());
    hub.record_and_broadcast(
        PANE,
        b"ENTERED".to_vec(),
        observed(true, true),
        Some(b"ALTSCREEN".to_vec()),
    );
    hub.record_and_broadcast(PANE, b"BACK".to_vec(), observed(false, true), None);

    let client = attach(&hub);
    let text = replay_text(&client);
    hub.stop();

    // The program's normal grid was preserved across the alternate screen, so
    // the frozen snapshot is as valid as when it left — and the alternate
    // screen's records are spent.
    assert!(
        text.ends_with("HISTORYSNAPSHOTBACK"),
        "the frozen record must come back exactly, got: {text:?}"
    );
    assert!(
        !text.contains("ALTSCREEN"),
        "the screen kept for the alternate buffer is spent, got: {text:?}"
    );
}
