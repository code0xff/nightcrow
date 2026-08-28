//! Rebuild a paste out of the key burst Windows delivers instead of one.
//!
//! The Windows console has no paste input record, so a paste arrives as plain
//! key presses whose `\r`s each submit a line. A synthetic `Event::Paste` hands
//! the burst back to the normal paste path. Compiled everywhere so the rule
//! stays one body and its tests run on every platform.

use crate::application::input::dispatch::has_command_modifier;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind};
use std::io;
use std::time::{Duration, Instant};

/// Bounds on one burst. A split paste is a correctness bug — each fragment
/// becomes its own bracketed block — so these are generous next to a realistic
/// paste; what is left over is drained on the next tick.
const MAX_BURST_EVENTS: usize = 8192;
const MAX_BURST_TIME: Duration = Duration::from_millis(250);

/// How long to wait for the burst to continue. The console feeds a paste in
/// incrementally, so a zero-wait poll finds the queue momentarily empty and
/// cuts the paste mid-word. Far below human cadence, so typing never merges.
const BURST_GAP: Duration = Duration::from_millis(5);

/// No one types this many characters at a 5 ms cadence, and key repeat cannot
/// either — its events carry `KeyEventKind::Repeat`.
const MAX_TYPED_CHARS: usize = 16;

/// Drain the rest of the burst `first` opened and classify it.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn coalesce_paste(first: Event) -> io::Result<Vec<Event>> {
    let mut events = vec![first];
    let deadline = Instant::now() + MAX_BURST_TIME;
    while events.len() < MAX_BURST_EVENTS && Instant::now() < deadline && event::poll(BURST_GAP)? {
        events.push(event::read()?);
    }
    Ok(classify(events))
}

pub(crate) fn classify(events: Vec<Event>) -> Vec<Event> {
    match paste_text(&events) {
        Some(text) => vec![Event::Paste(text)],
        // Returned untouched, order included: a chord must not be reordered
        // around the keys it modifies.
        None => events,
    }
}

/// The payload this burst would paste, or `None` if it reads as typing.
///
/// Narrow on purpose: a false positive submits typed keys as a block. Enter
/// hands the line off, so nothing typed can follow it within one burst — what
/// marks a paste is content the Enter did not submit.
fn paste_text(events: &[Event]) -> Option<String> {
    let mut text = String::new();
    let mut enters = 0usize;
    let mut chars = 0usize;
    let mut chars_after_enter = 0usize;
    for event in events {
        let Event::Key(key) = event else {
            return None;
        };
        // Windows reports Press/Repeat/Release per keystroke; counting the
        // others would multiply every character.
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match plain_input(*key)? {
            '\r' => {
                enters += 1;
                text.push('\r');
            }
            c => {
                chars += 1;
                if enters > 0 {
                    chars_after_enter += 1;
                }
                text.push(c);
            }
        }
    }
    let multiline = chars_after_enter > 0;
    if multiline || chars > MAX_TYPED_CHARS {
        Some(text)
    } else {
        None
    }
}

/// Enter becomes `\r`, which is what a terminal puts inside a bracketed paste.
/// `\n` is dropped outright by some readers (Claude Code), collapsing the lines.
fn plain_input(key: KeyEvent) -> Option<char> {
    // Shift is excluded deliberately — it carries the uppercase half of a paste.
    if has_command_modifier(key) {
        return None;
    }
    match key.code {
        KeyCode::Char(c) if !c.is_control() => Some(c),
        KeyCode::Enter => Some('\r'),
        _ => None,
    }
}
