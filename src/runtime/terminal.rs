use crate::backend::{BackendEvent, PaneId, TerminalBackend};
use std::collections::HashMap;

/// Upper bound on a pane's in-flight prompt buffer before further chars are
/// dropped. Prevents unbounded growth when a program writes a stream of bytes
/// without ever sending `\r` / `\n` (progress bars, large pastes, `yes` piped
/// to cat). 4 KiB easily exceeds any realistic shell prompt line.
const PROMPT_BUFFER_MAX_BYTES: usize = 4096;

/// Scrollback line cap for every vt100 parser. Lifted here so the terminal
/// state machine — which owns parser creation now — defines its own budget
/// rather than reading it from `app`.
pub const SCROLLBACK_LINES: usize = 1000;

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

/// Default count of panes shown side by side in the normal (non-fullscreen)
/// lower panel before the visible window starts sliding.
pub const MAX_VISIBLE_NORMAL: usize = 4;

/// Default count of panes shown side by side when the terminal panel is
/// fullscreen (still bounded by the F3–F9 direct-jump range).
pub const MAX_VISIBLE_FULLSCREEN: usize = 7;

/// Compute the visible pane-index window `[start, start+len)` for a split
/// grid capped at `max_visible` panes. `prev_start` is the previous window's
/// start (0 for a fresh terminal); the window is nudged the minimum amount
/// needed to keep `active` inside it, rather than re-centering every call —
/// so paging through panes one at a time doesn't reshuffle the whole grid.
/// Shared by `TerminalState::sync_visible_window` (state update) and
/// `ui::terminal_tab` (rendering) so both always agree on what's visible.
pub(crate) fn visible_range(
    prev_start: usize,
    active: usize,
    pane_count: usize,
    max_visible: usize,
) -> std::ops::Range<usize> {
    if pane_count == 0 || max_visible == 0 {
        return 0..0;
    }
    let window = max_visible.min(pane_count);
    let active = active.min(pane_count - 1);
    let max_start = pane_count - window;

    let mut start = prev_start.min(max_start);
    if active < start {
        start = active;
    } else if active >= start + window {
        start = active + 1 - window;
    }
    start..(start + window)
}

pub struct TerminalState {
    pub panes: Vec<PaneInfo>,
    pub active: usize,
    /// Default size used to create a pane before any layout resize has run
    /// (e.g. the very first pane on startup). Once a pane has a real content
    /// Rect, its size lives in `last_content_size` instead.
    pub size: (u16, u16),
    pub scroll: HashMap<PaneId, usize>,
    pub fullscreen: bool,
    /// Last (rows, cols) applied to each pane's backend + vt100 parser via
    /// `resize_visible_panes`. Panes currently scrolled out of the visible
    /// window keep whatever size they had when they were last visible.
    pub last_content_size: HashMap<PaneId, (u16, u16)>,
    /// Index of the first pane in the visible split-view window.
    pub visible_start: usize,
    pub max_visible_normal: usize,
    pub max_visible_fullscreen: usize,
    pub(crate) parsers: HashMap<PaneId, vt100::Parser<PaneCallbacks>>,
    pub(crate) prompt_bufs: HashMap<PaneId, String>,
    prompt_log_enabled: bool,
    pub(crate) backend: Option<Box<dyn TerminalBackend>>,
}

impl TerminalState {
    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.panes.get(self.active).map(|p| p.id)
    }

    /// Maximum number of panes shown at once in the current fullscreen state.
    pub fn max_visible(&self) -> usize {
        if self.fullscreen {
            self.max_visible_fullscreen
        } else {
            self.max_visible_normal
        }
    }

    /// Last known content size for `id`, falling back to the default pane
    /// size for a pane that hasn't been through a layout resize yet.
    pub fn pane_size(&self, id: PaneId) -> (u16, u16) {
        self.last_content_size
            .get(&id)
            .copied()
            .unwrap_or(self.size)
    }

    /// Row count used for terminal-scroll paging: the active pane's own
    /// content height when known, otherwise the default pane size. Callers
    /// used to read `size` directly, which no longer tracks per-pane height.
    pub fn active_pane_rows(&self) -> usize {
        self.active_pane_id()
            .map(|id| self.pane_size(id).0 as usize)
            .unwrap_or(self.size.0 as usize)
    }

    /// Re-clamp `visible_start` against the current active pane and pane
    /// count. Must be called after anything that changes `active` or
    /// `panes.len()` (focus jumps, pane create/close, session restore) so
    /// the split-view window always contains the active pane.
    pub fn sync_visible_window(&mut self) {
        let range = visible_range(
            self.visible_start,
            self.active,
            self.panes.len(),
            self.max_visible(),
        );
        self.visible_start = range.start;
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

    /// Resize each listed pane's backend PTY and vt100 parser to its own
    /// (rows, cols), skipping a pane whose size didn't change. `layouts`
    /// carries one entry per currently *visible* pane — panes scrolled out of
    /// the split-view window are omitted and keep their `last_content_size`
    /// until they become visible again.
    pub fn resize_visible_panes(&mut self, layouts: &[(PaneId, u16, u16)]) {
        let active_id = self.active_pane_id();
        for &(id, rows, cols) in layouts {
            let rows = rows.max(1);
            let cols = cols.max(1);
            if Some(id) == active_id {
                self.size = (rows, cols);
            }
            if self.last_content_size.get(&id) == Some(&(rows, cols)) {
                continue;
            }
            if let Some(backend) = &mut self.backend {
                backend.resize(id, rows, cols);
            }
            if let Some(parser) = self.parsers.get_mut(&id) {
                parser.screen_mut().set_size(rows, cols);
            }
            self.last_content_size.insert(id, (rows, cols));
        }
    }

    /// Byte payloads recorded by an underlying `FakeBackend`, for tests that
    /// assert exact PTY pass-through. `None` when the backend is not a
    /// `FakeBackend` (e.g. production `PtyBackend` or no backend).
    #[cfg(test)]
    pub(crate) fn fake_backend_sent(&self) -> Option<Vec<Vec<u8>>> {
        self.backend.as_ref().and_then(|b| b.test_sent_payloads())
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

    /// Drain pending backend events into vt100 parsers and pane metadata.
    /// Returns the pane ids the backend signalled as exited so the caller
    /// can run cross-cutting cleanup (focus redirect, fullscreen reset)
    /// that depends on state outside this struct.
    pub fn poll(&mut self) -> Vec<PaneId> {
        let mut exited = Vec::new();
        let events: Vec<BackendEvent> = self
            .backend
            .as_mut()
            .map(|b| b.drain_events())
            .unwrap_or_default();

        for event in events {
            match event {
                BackendEvent::Output { pane, data } => {
                    let new_title = if let Some(parser) = self.parsers.get_mut(&pane) {
                        parser.process(&data);
                        parser.callbacks_mut().pending_title.take()
                    } else {
                        None
                    };
                    if let Some(title) = new_title
                        && let Some(info) = self.panes.iter_mut().find(|p| p.id == pane)
                    {
                        info.title = title;
                    }
                }
                BackendEvent::Exited { pane } => {
                    // Single source of truth for pane removal: `drain_events`
                    // no longer touches the backend's pane map, so we drive
                    // the teardown here. `destroy_pane` is idempotent against
                    // a pane that `close_active` already removed.
                    if let Some(backend) = &mut self.backend {
                        backend.destroy_pane(pane);
                    }
                    self.remove_pane_state(pane);
                    self.panes.retain(|p| p.id != pane);
                    exited.push(pane);
                }
            }
        }
        exited
    }

    /// Allocate a new bare interactive-shell pane. Thin wrapper over
    /// `create_pane_with` for the common "open an empty terminal" path.
    pub fn create_pane(&mut self) -> anyhow::Result<()> {
        self.create_pane_with(None, None)
    }

    /// Allocate a new backend pane and matching vt100 parser. `command`, when
    /// present, is run in the pane's shell immediately; `label` sets the
    /// initial tab title (a program that emits OSC 0/2 can still override it
    /// later). Both default sensibly when `None`. The caller is expected to
    /// surface any error to the user.
    pub fn create_pane_with(
        &mut self,
        command: Option<&str>,
        label: Option<&str>,
    ) -> anyhow::Result<()> {
        // Seed the new pane with the active pane's current content size so it
        // starts roughly right-sized inside the split grid; the next frame's
        // `resize_visible_panes` corrects it to the actual cell Rect once the
        // pane count (and therefore the grid) has changed.
        let (rows, cols) = self
            .active_pane_id()
            .map(|id| self.pane_size(id))
            .unwrap_or(self.size);
        let rows = rows.max(1);
        let cols = cols.max(1);
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        let id = backend.create_pane(rows, cols, command)?;
        let parser = vt100::Parser::new_with_callbacks(
            rows,
            cols,
            SCROLLBACK_LINES,
            PaneCallbacks::default(),
        );
        self.parsers.insert(id, parser);
        self.last_content_size.insert(id, (rows, cols));
        // Title precedence: explicit label → command text → default shell N.
        let title = match (label, command) {
            (Some(l), _) if !l.trim().is_empty() => l.trim().to_string(),
            (_, Some(c)) if !c.trim().is_empty() => c.trim().to_string(),
            _ => format!("shell {}", self.panes.len() + 1),
        };
        self.panes.push(PaneInfo { id, title });
        self.active = self.panes.len() - 1;
        self.sync_visible_window();
        tracing::info!(pane = id, "terminal pane opened");
        Ok(())
    }

    /// Remove the currently active pane. Returns `true` when a pane was
    /// removed so the caller can re-clamp dependent state (focus,
    /// fullscreen). Returns `false` for an empty list — a benign no-op
    /// the caller can ignore.
    pub fn close_active(&mut self) -> bool {
        let Some(info) = self.panes.get(self.active) else {
            return false;
        };
        let id = info.id;
        tracing::info!(pane = id, "terminal pane closed");
        if let Some(backend) = &mut self.backend {
            backend.destroy_pane(id);
        }
        self.remove_pane_state(id);
        self.panes.remove(self.active);
        true
    }

    /// Screen for a specific pane, independent of which pane is currently
    /// active — the split-view renderer draws every visible pane, not just
    /// the focused one.
    pub fn screen_for_pane(&self, id: PaneId) -> Option<&vt100::Screen> {
        self.parsers.get(&id).map(vt100::Parser::screen)
    }

    pub fn active_screen(&self) -> Option<&vt100::Screen> {
        let id = self.active_pane_id()?;
        self.screen_for_pane(id)
    }

    fn remove_pane_state(&mut self, id: PaneId) {
        self.parsers.remove(&id);
        // Flush any unterminated prompt input so we don't lose the line the
        // user was composing when the pane closes.
        if let Some(buf) = self.prompt_bufs.remove(&id)
            && !buf.is_empty()
        {
            tracing::info!(target: "prompt", pane = id, text = %buf);
        }
        self.scroll.remove(&id);
        self.last_content_size.remove(&id);
    }

    pub fn new(backend: Option<Box<dyn TerminalBackend>>, prompt_log_enabled: bool) -> Self {
        Self {
            panes: Vec::new(),
            active: 0,
            size: (22, 78),
            scroll: HashMap::new(),
            fullscreen: false,
            last_content_size: HashMap::new(),
            visible_start: 0,
            max_visible_normal: MAX_VISIBLE_NORMAL,
            max_visible_fullscreen: MAX_VISIBLE_FULLSCREEN,
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
/// malformed sequence isn't accidentally eaten — and leave that control byte
/// in the iterator: eating it here would silently drop a `\n` or `\r` that
/// the outer pass needs to flush the prompt buffer. DEL (0x7f) is treated
/// per ECMA-48 as a no-op inside the sequence: consumed but does not stand
/// in for a final byte.
fn consume_csi(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    while let Some(&c) = chars.peek() {
        if c < '\x20' {
            return;
        }
        chars.next();
        if c == '\x7f' {
            continue;
        }
        if ('\x40'..='\x7e').contains(&c) {
            return;
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
    fn consume_csi_skips_del_byte_per_ecma48() {
        // ESC [ 3 1 DEL m sgr — the DEL must be ignored without terminating
        // the sequence early. The trailing 'm' is the real final byte; the
        // following "ok" should survive intact.
        let out = strip_escape_sequences(b"\x1b[31\x7fmok");
        assert_eq!(out, "ok");
    }

    #[test]
    fn strip_escape_sequences_preserves_newline_after_malformed_csi() {
        // A CSI body interrupted by a control byte must leave that byte for
        // the outer pass so prompt-buffer flush on `\n` still fires.
        let out = strip_escape_sequences(b"\x1b[31\ndone\n");
        assert_eq!(out, "\ndone\n");
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

    fn state_with_fake() -> TerminalState {
        let backend = Box::new(crate::test_util::FakeBackend::default());
        TerminalState::new(Some(backend), false)
    }

    #[test]
    fn create_pane_defaults_to_shell_label_and_no_command() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        assert_eq!(state.panes.len(), 1);
        assert_eq!(state.panes[0].title, "shell 1");
    }

    #[test]
    fn create_pane_with_label_sets_title() {
        let mut state = state_with_fake();
        state
            .create_pane_with(Some("claude --foo"), Some("Claude"))
            .unwrap();
        assert_eq!(state.panes[0].title, "Claude");
    }

    #[test]
    fn create_pane_with_falls_back_to_command_text() {
        let mut state = state_with_fake();
        state.create_pane_with(Some("cargo test"), None).unwrap();
        assert_eq!(state.panes[0].title, "cargo test");
    }

    #[test]
    fn create_pane_with_appends_and_focuses_new_pane() {
        let mut state = state_with_fake();
        state.create_pane_with(Some("echo hi"), Some("E")).unwrap();
        state.create_pane().unwrap();
        assert_eq!(state.panes.len(), 2);
        assert_eq!(state.panes[1].title, "shell 2");
        assert_eq!(state.active, 1);
    }

    #[test]
    fn pane_size_falls_back_to_default_before_any_resize() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let id = state.panes[0].id;
        assert_eq!(state.pane_size(id), state.size);
    }

    #[test]
    fn resize_visible_panes_updates_parser_and_last_content_size() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let id = state.panes[0].id;

        state.resize_visible_panes(&[(id, 12, 60)]);

        assert_eq!(state.screen_for_pane(id).unwrap().size(), (12, 60));
        assert_eq!(state.last_content_size.get(&id), Some(&(12, 60)));
    }

    #[test]
    fn resize_visible_panes_clamps_zero_to_one() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let id = state.panes[0].id;

        state.resize_visible_panes(&[(id, 0, 0)]);

        assert_eq!(state.last_content_size.get(&id), Some(&(1, 1)));
    }

    #[test]
    fn resize_visible_panes_ignores_panes_not_listed() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let hidden_id = state.panes[0].id;
        let hidden_size_at_creation = state.pane_size(hidden_id);
        state.create_pane().unwrap();
        let visible_id = state.panes[1].id;

        state.resize_visible_panes(&[(visible_id, 15, 70)]);

        // The hidden pane keeps whatever size it had before this call — it
        // wasn't in the `layouts` list, so `resize_visible_panes` must not
        // touch it.
        assert_eq!(
            state.last_content_size.get(&hidden_id),
            Some(&hidden_size_at_creation)
        );
        assert_eq!(state.last_content_size.get(&visible_id), Some(&(15, 70)));
    }

    #[test]
    fn new_pane_seeds_size_from_active_pane_last_content_size() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let first_id = state.panes[0].id;
        state.resize_visible_panes(&[(first_id, 18, 65)]);

        state.create_pane().unwrap();
        let second_id = state.panes[1].id;

        assert_eq!(state.screen_for_pane(second_id).unwrap().size(), (18, 65));
    }

    #[test]
    fn screen_for_pane_none_for_unknown_id() {
        let state = state_with_fake();
        assert!(state.screen_for_pane(999).is_none());
    }

    #[test]
    fn closing_pane_drops_its_last_content_size() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let id = state.panes[0].id;
        state.resize_visible_panes(&[(id, 10, 40)]);

        state.close_active();

        assert!(!state.last_content_size.contains_key(&id));
    }

    #[test]
    fn max_visible_switches_with_fullscreen() {
        let mut state = state_with_fake();
        state.max_visible_normal = 4;
        state.max_visible_fullscreen = 7;
        assert_eq!(state.max_visible(), 4);
        state.fullscreen = true;
        assert_eq!(state.max_visible(), 7);
    }

    #[test]
    fn visible_range_shows_everything_under_the_cap() {
        assert_eq!(visible_range(0, 0, 3, 4), 0..3);
    }

    #[test]
    fn visible_range_keeps_active_inside_a_capped_window() {
        // 7 panes, window of 4, active is the last pane: window must end at 7.
        assert_eq!(visible_range(0, 6, 7, 4), 3..7);
    }

    #[test]
    fn visible_range_moves_start_forward_only_as_far_as_needed() {
        // Previously showing [2,6). Active moves to 6 (just past the window):
        // start should shift by exactly 1, not jump to re-center.
        assert_eq!(visible_range(2, 6, 7, 4), 3..7);
    }

    #[test]
    fn visible_range_moves_start_backward_when_active_precedes_window() {
        // Previously showing [3,7). Active jumps back to 0.
        assert_eq!(visible_range(3, 0, 7, 4), 0..4);
    }

    #[test]
    fn visible_range_empty_when_no_panes() {
        assert_eq!(visible_range(0, 0, 0, 4), 0..0);
    }

    #[test]
    fn sync_visible_window_follows_active_when_panes_exceed_max_visible() {
        let mut state = state_with_fake();
        state.max_visible_normal = 4;
        for i in 0..7 {
            state
                .create_pane_with(None, Some(&format!("P{i}")))
                .unwrap();
        }
        // Each create_pane_with call makes the new pane active and syncs the
        // window, so after 7 panes the last one (index 6) must be visible.
        assert_eq!(state.active, 6);
        assert!(state.visible_start <= 6 && state.visible_start + 4 > 6);
    }

    #[test]
    fn sync_visible_window_clamps_after_pane_count_shrinks() {
        let mut state = state_with_fake();
        state.max_visible_normal = 4;
        for i in 0..7 {
            state
                .create_pane_with(None, Some(&format!("P{i}")))
                .unwrap();
        }
        // Window is currently sliding near the end; drop back to a single
        // pane and re-sync — start must fall back inside [0, 0].
        state.panes.truncate(1);
        state.active = 0;
        state.sync_visible_window();
        assert_eq!(state.visible_start, 0);
    }

    #[test]
    fn active_pane_rows_uses_pane_specific_size() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let id = state.panes[0].id;
        state.resize_visible_panes(&[(id, 33, 90)]);
        assert_eq!(state.active_pane_rows(), 33);
    }

    #[test]
    fn resize_visible_panes_keeps_default_size_in_sync_with_active_pane() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let first_id = state.panes[0].id;
        state.create_pane().unwrap();
        let second_id = state.panes[1].id;
        state.active = 1;

        state.resize_visible_panes(&[(first_id, 10, 40), (second_id, 12, 50)]);

        assert_eq!(state.size, (12, 50));
    }

    #[test]
    fn active_pane_rows_falls_back_to_default_with_no_panes() {
        let state = state_with_fake();
        assert_eq!(state.active_pane_rows(), state.size.0 as usize);
    }
}
