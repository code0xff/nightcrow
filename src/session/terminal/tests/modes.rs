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
