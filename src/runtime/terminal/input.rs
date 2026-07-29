//! Sending what the user typed to the active pane, and keeping the prompt log.
//!
//! Bytes go to the pane untouched; the prompt log is a side record of the lines
//! typed at a shell, reconstructed here because nothing downstream sees them as
//! lines — the pane sees keystrokes and the program sees a terminal.

use crate::backend::PaneId;
use crate::runtime::terminal::{PROMPT_BUFFER_MAX_BYTES, TerminalState, strip_escape_sequences};

impl TerminalState {
    pub fn send_input(&mut self, data: &[u8]) {
        let Some(info) = self.panes.get(self.active) else {
            return;
        };
        let id = info.id;
        self.scroll.remove(&id);
        if let Some(backend) = &mut self.backend
            && let Err(e) = backend.send_input(id, data)
        {
            tracing::warn!("failed to send terminal input to pane {id}: {e}");
        }
        if self.prompt_log_enabled {
            self.buffer_prompt_input(id, data);
        }
    }

    pub(super) fn buffer_prompt_input(&mut self, pane_id: PaneId, data: &[u8]) {
        let text = strip_escape_sequences(data);
        let buf = self.prompt_bufs.entry(pane_id).or_default();
        for ch in text.chars() {
            match ch {
                '\r' | '\n' => {
                    if !buf.is_empty() {
                        tracing::info!(target: "prompt", pane = pane_id, text = %buf);
                        buf.clear();
                    }
                }
                // 0x7f (DEL, sent by Backspace) and 0x08 (BS, sent by Ctrl+H)
                // both remove the previous typed char. Without this branch the
                // prompt log would accumulate typos the user already corrected.
                '\x7f' | '\x08' => {
                    buf.pop();
                }
                _ => {
                    // Cap to bound memory under degenerate "no-newline" producers
                    // (progress bars piped through cat, paste of a multi-MB
                    // string, etc.). Dropping further chars before the next flush
                    // is preferable to letting the buffer grow without limit.
                    if buf.len() < PROMPT_BUFFER_MAX_BYTES {
                        buf.push(ch);
                    }
                }
            }
        }
    }
}
