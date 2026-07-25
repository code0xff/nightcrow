//! Wire protocol for the web mirror: server→browser screen frames and
//! browser→server input events.
//!
//! Output re-uses ratatui's own `CrosstermBackend`, so the bytes are
//! byte-identical to what the local terminal receives, and each chunk
//! self-terminates with a style reset so chunks concatenate cleanly on a
//! single xterm.js instance. Input decodes a small JSON envelope into
//! crossterm events, so browser input runs through the exact same routing as
//! local input — a web action can never diverge from the equivalent keypress.

use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::Position;

/// A decoded browser input event, already lowered to the crossterm types the
/// local input path consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebInputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Paste(String),
}

/// Encode a full repaint of `current` for a freshly connected client. Clears
/// first so a reconnecting xterm.js drops stale content, then paints every
/// non-blank cell.
pub fn encode_full_frame(current: &Buffer) -> Vec<u8> {
    let blank = Buffer::empty(*current.area());
    let updates = blank.diff(current);
    let mut out = Vec::new();
    // Hide the cursor for the duration of the repaint so it doesn't chase the
    // painted cells; `encode_cursor` re-shows it at the right spot.
    out.extend_from_slice(b"\x1b[?25l");
    {
        let mut backend = CrosstermBackend::new(&mut out);
        // `clear` + `draw` both write ANSI into the Vec; flush on a Vec is a
        // no-op and neither can fail on an in-memory writer.
        let _ = backend.clear();
        let _ = backend.draw(updates.into_iter());
    }
    out
}

/// Encode the incremental update needed to bring a client from `previous` to
/// `current`. Returns empty when nothing changed or the buffers differ in
/// dimensions — a size change is not a cell-level diff and needs a full frame.
pub fn encode_update(previous: &Buffer, current: &Buffer) -> Vec<u8> {
    if previous.area() != current.area() {
        return Vec::new();
    }
    let updates = previous.diff(current);
    if updates.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    {
        let mut backend = CrosstermBackend::new(&mut out);
        let _ = backend.draw(updates.into_iter());
    }
    out
}

/// Encode the trailing cursor state for a frame chunk. The cell buffer carries
/// no cursor, so every chunk ends with an explicit park: move+show, or hide
/// when the frame has no cursor. Coordinates are absolute screen cells; ANSI
/// is 1-based, the buffer 0.
pub fn encode_cursor(cursor: Option<Position>) -> Vec<u8> {
    match cursor {
        Some(p) => format!("\x1b[{};{}H\x1b[?25h", p.y as u32 + 1, p.x as u32 + 1).into_bytes(),
        None => b"\x1b[?25l".to_vec(),
    }
}

mod decode;

#[cfg(test)]
pub use decode::MAX_INPUT_MESSAGE_BYTES;
pub use decode::{decode_input, ensure_input_size};

#[cfg(test)]
mod tests;
