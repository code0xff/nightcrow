//! Keys while the help overlay is up.
//!
//! The overlay is a reading surface, so it binds only what reading needs and
//! swallows everything else: a key that fell through to the project underneath
//! would act on a screen the user cannot see.

use super::dispatch::KeyOutcome;
use crate::workspace::Workspace;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Lines a page key moves. Fixed rather than the popup's height because the
/// input path never learns the frame size; the render clamps the result.
const PAGE_STEP: i16 = 10;

pub(super) fn handle_help_key(ws: &mut Workspace, key: KeyEvent) -> KeyOutcome {
    let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl_c {
        ws.help.close();
        return KeyOutcome::Continue;
    }
    match key.code {
        // Closing keys, including the two that opened it: the same chord that
        // raised the overlay puts it away.
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | '?' | 'h') => ws.help.close(),
        KeyCode::Char('j') | KeyCode::Down => ws.help.scroll_by(1),
        KeyCode::Char('k') | KeyCode::Up => ws.help.scroll_by(-1),
        KeyCode::PageDown | KeyCode::Char(' ') => ws.help.scroll_by(PAGE_STEP),
        KeyCode::PageUp => ws.help.scroll_by(-PAGE_STEP),
        KeyCode::Home => ws.help.scroll_by(i16::MIN),
        _ => {}
    }
    KeyOutcome::Continue
}

#[cfg(test)]
#[path = "help_tests.rs"]
mod tests;
