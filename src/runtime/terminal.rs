use crate::backend::{PaneId, TerminalBackend};
use std::collections::HashMap;

pub struct PaneInfo {
    pub id: PaneId,
    pub title: String,
}

/// vt100 callbacks that capture OSC 0/2 window title updates so the tab bar
/// can reflect what the running program (claude, vim, ssh, …) advertises.
/// Bare shells without precmd hooks never emit OSC, so a sensible default
/// title still lives on `PaneInfo`.
#[derive(Default, Debug)]
pub(crate) struct PaneCallbacks {
    pub(crate) pending_title: Option<String>,
}

impl vt100::Callbacks for PaneCallbacks {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        let cleaned: String = String::from_utf8_lossy(title)
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        let trimmed = cleaned.trim();
        if !trimmed.is_empty() {
            self.pending_title = Some(trimmed.to_string());
        }
    }
}

pub struct TerminalState {
    pub panes: Vec<PaneInfo>,
    pub active: usize,
    pub size: (u16, u16),
    pub scroll: HashMap<PaneId, usize>,
    pub fullscreen: bool,
    pub(crate) parsers: HashMap<PaneId, vt100::Parser<PaneCallbacks>>,
    pub(crate) prompt_bufs: HashMap<PaneId, String>,
    prompt_log_enabled: bool,
    pub(crate) backend: Option<Box<dyn TerminalBackend>>,
}

impl TerminalState {
    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.panes.get(self.active).map(|p| p.id)
    }

    pub fn scroll_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(id) = self.active_pane_id() {
            let offset = self.scroll.entry(id).or_insert(0);
            *offset = offset.saturating_add(lines);
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(id) = self.active_pane_id()
            && let Some(entry) = self.scroll.get_mut(&id)
        {
            *entry = entry.saturating_sub(lines);
            if *entry == 0 {
                self.scroll.remove(&id);
            }
        }
    }

    pub fn is_scrolled(&self) -> bool {
        self.active_pane_id()
            .and_then(|id| self.scroll.get(&id))
            .is_some_and(|&v| v > 0)
    }

    pub fn sync_scroll(&mut self) {
        let Some(id) = self.active_pane_id() else {
            return;
        };
        let offset = self.scroll.get(&id).copied().unwrap_or(0);
        let actual = match self.parsers.get_mut(&id) {
            Some(parser) => {
                // vt100 clamps the offset to the actual scrollback
                // buffer size internally, so we can pass the full request
                // through and read back what was applied.
                parser.screen_mut().set_scrollback(offset);
                parser.screen().scrollback()
            }
            None => return,
        };
        if actual == 0 {
            self.scroll.remove(&id);
        } else {
            self.scroll.insert(id, actual);
        }
    }

    fn buffer_prompt_input(&mut self, pane_id: PaneId, data: &[u8]) {
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
                _ => buf.push(ch),
            }
        }
    }

    pub fn resize_panes(&mut self, rows: u16, cols: u16) {
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        let r = rows.max(1);
        let c = cols.max(1);
        for info in &self.panes {
            if let Some(backend) = &mut self.backend {
                backend.resize(info.id, r, c);
            }
            if let Some(parser) = self.parsers.get_mut(&info.id) {
                parser.screen_mut().set_size(r, c);
            }
        }
    }

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

    pub fn new(backend: Option<Box<dyn TerminalBackend>>, prompt_log_enabled: bool) -> Self {
        Self {
            panes: Vec::new(),
            active: 0,
            size: (22, 78),
            scroll: HashMap::new(),
            fullscreen: false,
            parsers: HashMap::new(),
            prompt_bufs: HashMap::new(),
            prompt_log_enabled,
            backend,
        }
    }
}

pub(crate) fn strip_escape_sequences(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => consume_escape_sequence(&mut chars),
            // \r, \n, and the line-editing controls (BS, DEL) are forwarded
            // so `buffer_prompt_input` can flush on newlines and pop on
            // backspace; every other control byte is dropped.
            '\r' | '\n' | '\x08' | '\x7f' => result.push(ch),
            c if !c.is_control() => result.push(c),
            _ => {}
        }
    }
    result
}

/// Consume the body of an ESC-introduced control sequence. Called with the
/// leading ESC already taken; advances `chars` past the sequence's terminator
/// (or leaves the iterator alone for a bare ESC).
fn consume_escape_sequence(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    match chars.peek().copied() {
        Some('[') => {
            chars.next();
            consume_csi(chars);
        }
        Some(']') => {
            chars.next();
            consume_osc(chars);
        }
        Some('O') => {
            chars.next();
            consume_ss3(chars);
        }
        Some('(') | Some(')') | Some('*') | Some('+') | Some('-') | Some('.') | Some('/')
        | Some('#') => {
            // Charset designators / DEC private 2-byte escapes:
            // ESC <intermediate> <final>. Skip both.
            chars.next();
            chars.next();
        }
        _ => {
            // Drop the bare ESC and let the next iteration process whatever
            // follows as ordinary input. Consuming an extra byte here would
            // silently swallow user keystrokes that happened to land right
            // after a stray Esc.
        }
    }
}

/// CSI: consume parameter/intermediate bytes (0x20–0x3f), stop at the final
/// byte (0x40–0x7e). Break early on a control char so content that follows a
/// malformed sequence isn't accidentally eaten.
fn consume_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    for c in chars.by_ref() {
        if ('\x40'..='\x7e').contains(&c) || c < '\x20' {
            break;
        }
    }
}

/// OSC: skip until BEL (0x07) or ST (ESC \).
fn consume_osc(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    loop {
        match chars.next() {
            None | Some('\x07') => break,
            Some('\x1b') if chars.peek() == Some(&'\\') => {
                chars.next();
                break;
            }
            _ => {}
        }
    }
}

/// SS3: ESC O <final>. Used by xterm-style application keypad for arrow/
/// function keys. Consume the next char only when it looks like a valid SS3
/// final byte (0x40–0x7e) — a malformed `ESC O <x>` sequence used to swallow
/// the following ordinary char.
fn consume_ss3(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    if let Some(&next) = chars.peek()
        && ('\x40'..='\x7e').contains(&next)
    {
        chars.next();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parser() -> vt100::Parser<PaneCallbacks> {
        vt100::Parser::new_with_callbacks(3, 20, 0, PaneCallbacks::default())
    }

    #[test]
    fn captures_osc_two_window_title() {
        let mut p = parser();
        p.process(b"\x1b]2;claude\x07");
        assert_eq!(p.callbacks().pending_title.as_deref(), Some("claude"));
    }

    #[test]
    fn captures_osc_zero_title_and_strips_controls() {
        let mut p = parser();
        // OSC 0 sets both icon name and window title; embedded tab/BS bytes
        // must not leak into the tab label.
        p.process(b"\x1b]0;cargo\t test\x08\x07");
        assert_eq!(p.callbacks().pending_title.as_deref(), Some("cargo test"));
    }

    #[test]
    fn ignores_empty_title() {
        let mut p = parser();
        p.process(b"\x1b]2;\x07");
        assert!(p.callbacks().pending_title.is_none());
    }

    #[test]
    fn later_title_replaces_earlier_until_taken() {
        let mut p = parser();
        p.process(b"\x1b]2;first\x07");
        p.process(b"\x1b]2;second\x07");
        let taken = p.callbacks_mut().pending_title.take();
        assert_eq!(taken.as_deref(), Some("second"));
        assert!(p.callbacks().pending_title.is_none());
    }
}
