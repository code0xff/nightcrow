//! What the hub reads out of a pane's output stream on everyone's behalf.
//!
//! Straight at the tracker, with no shell and no PTY: the rules here are about
//! parsing bytes, and putting a real program in front of them would only add a
//! deadline to wait on. The end of this — that a client which was not there is
//! told the answer — is asserted in [`reattach`](super::reattach).

use crate::session::limits::MAX_PANE_TITLE_CHARS;
use crate::session::terminal::hub_modes::PaneModeTracker;

const PANE: crate::backend::PaneId = 1;
const SIZE: (u16, u16) = (24, 80);

fn observe(data: &str) -> Option<String> {
    PaneModeTracker::default()
        .observe(PANE, data.as_bytes(), || SIZE)
        .title
}

#[test]
fn a_pane_that_titles_itself_is_heard() {
    // OSC 2 (title) and OSC 0 (icon and title) both name the tab.
    assert_eq!(observe("\x1b]2;agent\x07").as_deref(), Some("agent"));
    assert_eq!(observe("\x1b]0;agent\x07").as_deref(), Some("agent"));
}

#[test]
fn output_that_sets_no_title_says_nothing_about_it() {
    // `None` has to mean "unchanged" rather than "cleared" -- every chunk of a
    // pane's output comes through here, and almost none of them carry a title.
    assert_eq!(observe("just some output\r\n"), None);
}

/// A program that emits an empty title means "leave it alone", and one that
/// asks for whitespace has not named anything either.
#[test]
fn a_title_that_is_empty_or_blank_does_not_take_effect() {
    assert_eq!(observe("\x1b]2;\x07"), None);
    assert_eq!(observe("\x1b]2;   \x07"), None);
}

#[test]
fn the_last_title_in_a_chunk_is_the_one_that_counts() {
    assert_eq!(
        observe("\x1b]2;first\x07 work \x1b]2;second\x07").as_deref(),
        Some("second")
    );
}

/// The child process chooses this string and every connecting client is handed
/// it, so it is bounded on the way in rather than wherever it is drawn.
#[test]
fn a_title_longer_than_the_cap_is_cut_to_it() {
    let long = "t".repeat(MAX_PANE_TITLE_CHARS + 50);
    let title = observe(&format!("\x1b]2;{long}\x07")).expect("the pane named itself");

    assert_eq!(title.chars().count(), MAX_PANE_TITLE_CHARS);
}

/// Counted in characters, not bytes: cutting a multi-byte character in half
/// would leave the tab a replacement glyph, and `String` cannot be sliced there
/// at all.
#[test]
fn a_multibyte_title_is_cut_on_a_character_boundary() {
    let long = "가".repeat(MAX_PANE_TITLE_CHARS + 50);
    let title = observe(&format!("\x1b]2;{long}\x07")).expect("the pane named itself");

    assert_eq!(title.chars().count(), MAX_PANE_TITLE_CHARS);
    assert!(title.chars().all(|c| c == '가'));
}

/// The worker records an alternate-screen pane as a screen and a normal-screen
/// pane as a byte ring, so the moment a pane crosses between them has to be
/// reported with the chunk that crossed it — discovering it a tick later would
/// leave bytes filed against the wrong record.
#[test]
fn the_chunk_that_changes_which_screen_a_pane_is_on_says_so() {
    let mut tracker = PaneModeTracker::default();

    let first = tracker.observe(PANE, b"plain", || SIZE);
    assert!(
        !first.alt_changed && !first.modes.alt_screen,
        "a pane starts on the normal screen, so nothing changed"
    );

    let entering = tracker.observe(PANE, b"\x1b[?1049h", || SIZE);
    assert!(entering.alt_changed && entering.modes.alt_screen);

    let staying = tracker.observe(PANE, b"painting", || SIZE);
    assert!(!staying.alt_changed && staying.modes.alt_screen);

    let leaving = tracker.observe(PANE, b"\x1b[?1049l", || SIZE);
    assert!(leaving.alt_changed && !leaving.modes.alt_screen);
}

/// A pane whose process is gone takes its emulator *and* its remembered mode
/// with it. Leaving the flag behind would make the first chunk of whatever
/// reuses the id report no change from a screen that pane was never on.
#[test]
fn a_forgotten_pane_is_back_to_starting_on_the_normal_screen() {
    let mut tracker = PaneModeTracker::default();
    tracker.observe(PANE, b"\x1b[?1049h", || SIZE);
    tracker.forget(PANE);

    let fresh = tracker.observe(PANE, b"\x1b[?1049h", || SIZE);
    assert!(
        fresh.alt_changed,
        "entering the alternate screen again must read as a change"
    );
}

/// The grid a snapshot is read from has to be the size the child is drawing at,
/// or a connecting client is handed a screen laid out differently from every
/// other client's. Measured through the snapshot because that is what reads the
/// grid: it positions every row, so the last row it names is the row count.
#[test]
fn a_resized_pane_is_snapshotted_at_its_new_size() {
    let mut tracker = PaneModeTracker::default();
    tracker.observe(PANE, b"\x1b[?1049hpainted", || SIZE);
    assert!(last_row_of_snapshot(&tracker) == 24, "the fixture's size");

    tracker.resize(PANE, 10, 40);
    tracker.observe(PANE, b"more", || SIZE);

    assert_eq!(last_row_of_snapshot(&tracker), 10);
}

/// The highest row the snapshot positions, which is its row count: it starts
/// every row with that row's `CUP`.
fn last_row_of_snapshot(tracker: &PaneModeTracker) -> u16 {
    let snapshot = tracker.snapshot(PANE).expect("the pane has an emulator");
    let text = String::from_utf8_lossy(&snapshot);
    (1..=crate::session::limits::MAX_PANE_ROWS)
        .filter(|row| text.contains(&format!("\x1b[{row};1H")))
        .max()
        .expect("a snapshot positions every row")
}

/// A pane that has produced nothing has no emulator, so there is no screen to
/// hand anyone — and asking must not conjure one at a size nobody chose.
#[test]
fn a_pane_that_has_produced_nothing_has_no_screen() {
    assert!(PaneModeTracker::default().snapshot(PANE).is_none());
}
