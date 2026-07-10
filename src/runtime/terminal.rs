use crate::backend::{BackendEvent, PaneId, TerminalBackend};
use crate::input::{encode_arrow, encode_button, encode_wheel, encode_wheel_horizontal};
use crate::runtime::emulator::{PaneEmulator, ScreenView, ScrollSink};
use crossterm::event::MouseButton;
use std::collections::HashMap;

/// Upper bound on a pane's in-flight prompt buffer before further chars are
/// dropped. Prevents unbounded growth when a program writes a stream of bytes
/// without ever sending `\r` / `\n` (progress bars, large pastes, `yes` piped
/// to cat). 4 KiB easily exceeds any realistic shell prompt line.
const PROMPT_BUFFER_MAX_BYTES: usize = 4096;

/// Scrollback line cap for every pane emulator. Lifted here so the terminal
/// state machine — which owns emulator creation now — defines its own budget
/// rather than reading it from `app`.
pub const SCROLLBACK_LINES: usize = 1000;

/// Lines moved by a single line-scroll keypress (`Shift+Up`/`Shift+Down`).
pub const SCROLL_LINE_STEP: usize = 3;

/// Lines one mouse wheel notch scrolls, by terminal convention. Used to
/// convert a line count into a notch count when a pane wants wheel events,
/// and by the mouse handler as the line count of one captured wheel event.
pub const WHEEL_LINES_PER_NOTCH: usize = 3;

pub struct PaneInfo {
    pub id: PaneId,
    pub title: String,
}

/// Default count of panes shown side by side in the normal (non-fullscreen)
/// lower panel before the visible window starts sliding.
pub const MAX_VISIBLE_NORMAL: usize = 4;

/// Default count of panes shown side by side when the terminal panel is
/// fullscreen (still bounded by the F3–F10 direct-jump range).
pub const MAX_VISIBLE_FULLSCREEN: usize = 8;

/// Fullscreen state of the lower terminal panel. `<leader> f` cycles through
/// these while the terminal is focused: `Off → Grid → Zoom → Off`.
/// - `Off`: normal split — top viewer above, terminal split-view below.
/// - `Grid`: terminal fills the body; up to `MAX_VISIBLE_FULLSCREEN` panes.
/// - `Zoom`: terminal fills the body showing only the active pane. Rendered
///   by the same grid path with a visible cap of 1, so no dedicated render
///   branch is needed.
///
/// `Grid` and `Zoom` are visually identical whenever `Grid` would show a
/// single pane, so the cycle skips `Zoom` in that case (see
/// `TerminalState::zoom_distinct_from_grid` and
/// `App::toggle_terminal_fullscreen`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TerminalFullscreen {
    #[default]
    Off,
    Grid,
    Zoom,
}

impl TerminalFullscreen {
    /// Whether the terminal panel takes over the whole body area (both `Grid`
    /// and `Zoom`), hiding the top diff/list viewer.
    pub fn fills_body(self) -> bool {
        !matches!(self, TerminalFullscreen::Off)
    }
}

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
    pub fullscreen: TerminalFullscreen,
    /// Last (rows, cols) applied to each pane's backend + emulator via
    /// `resize_visible_panes`. Panes currently scrolled out of the visible
    /// window keep whatever size they had when they were last visible.
    pub last_content_size: HashMap<PaneId, (u16, u16)>,
    /// Index of the first pane in the visible split-view window.
    pub visible_start: usize,
    pub max_visible_normal: usize,
    pub max_visible_fullscreen: usize,
    pub(crate) emulators: HashMap<PaneId, PaneEmulator>,
    pub(crate) prompt_bufs: HashMap<PaneId, String>,
    prompt_log_enabled: bool,
    pub(crate) backend: Option<Box<dyn TerminalBackend>>,
}

impl TerminalState {
    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.panes.get(self.active).map(|p| p.id)
    }

    /// Maximum number of panes shown at once in the current fullscreen state.
    /// `Zoom` caps at 1 so the shared grid path renders only the active pane.
    pub fn max_visible(&self) -> usize {
        match self.fullscreen {
            TerminalFullscreen::Off => self.max_visible_normal,
            TerminalFullscreen::Grid => self.max_visible_fullscreen,
            TerminalFullscreen::Zoom => 1,
        }
    }

    /// Whether `Zoom` would render differently from `Grid` — i.e. whether
    /// `Grid` would show more than one pane. When false the two are
    /// indistinguishable, so the fullscreen cycle skips `Zoom` and a pane
    /// close normalizes `Zoom` back to `Grid`. Guards against both a lone pane
    /// and a `max_visible_fullscreen` of 1, so no site has to assume the cap
    /// is ≥ 2.
    pub fn zoom_distinct_from_grid(&self) -> bool {
        self.max_visible_fullscreen.min(self.panes.len()) > 1
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

    /// Scroll the active pane by `lines`. See `scroll_pane`.
    pub fn scroll_active(&mut self, up: bool, lines: usize) {
        let Some(id) = self.active_pane_id() else {
            return;
        };
        self.scroll_pane(id, up, lines, None);
    }

    /// Scroll pane `id` by `lines`, delivering the request wherever that
    /// pane's program expects it (see `ScrollSink`). `pointer` is the 1-based
    /// pane-local cell of a captured mouse wheel event, when there is one.
    ///
    /// Only the `Scrollback` sink moves the emulator's view; the other two
    /// synthesize input, because a program that owns its viewport keeps its
    /// transcript out of the emulator's grid entirely and scrolling the grid
    /// would reveal nothing.
    pub fn scroll_pane(&mut self, id: PaneId, up: bool, lines: usize, pointer: Option<(u16, u16)>) {
        if lines == 0 {
            return;
        }
        let Some(emulator) = self.emulators.get(&id) else {
            return;
        };
        let sink = emulator.scroll_sink();
        let app_cursor = emulator.app_cursor();

        match sink {
            ScrollSink::MouseWheel => {
                // A TUI may pick which of its regions to scroll from the
                // report's coordinates, so a captured wheel event passes the
                // real pointer cell through. Keyboard scrolls have no
                // pointer and report the pane's centre instead — the only
                // cell guaranteed to be inside the transcript rather than on
                // a border or input box.
                let (col, row) = match pointer {
                    Some(cell) => cell,
                    None => {
                        let (rows, cols) = self.pane_size(id);
                        (cols / 2 + 1, rows / 2 + 1)
                    }
                };
                let notch = encode_wheel(up, col, row);
                let notches = lines.div_ceil(WHEEL_LINES_PER_NOTCH);
                let payload = notch.repeat(notches);
                self.write_pty(id, &payload);
            }
            ScrollSink::ArrowKeys => {
                let payload = encode_arrow(up, app_cursor).repeat(lines);
                self.write_pty(id, &payload);
            }
            ScrollSink::Scrollback => {
                if up {
                    self.scroll_up(id, lines);
                } else {
                    self.scroll_down(id, lines);
                }
                // A wheel event can target a non-active pane, which the
                // per-frame `sync_scroll` (active pane only) never reaches —
                // apply the offset here so the view moves immediately.
                self.sync_scroll_pane(id);
            }
        }
    }

    /// Forward a horizontal wheel notch to pane `id` as an SGR report at the
    /// pointer cell. Horizontal scrolling has no scrollback or arrow-key
    /// analog, so there is no sink dispatch: a pane whose program asked for
    /// wheel reports receives the notch, every other pane silently drops it
    /// (the same rule as `click_pane`).
    pub fn wheel_horizontal_pane(&mut self, id: PaneId, left: bool, col: u16, row: u16) {
        let Some(emulator) = self.emulators.get(&id) else {
            return;
        };
        if emulator.scroll_sink() != ScrollSink::MouseWheel {
            return;
        }
        let payload = encode_wheel_horizontal(left, col, row);
        self.write_pty(id, &payload);
    }

    /// Forward a mouse button press or release to pane `id`, translated to an
    /// SGR report at 1-based pane-local `col`/`row`. Only a pane whose program
    /// asked for SGR mouse reports receives anything: a click has no
    /// scrollback fallback, so an unclaimed click is dropped — the same
    /// silence rule that keeps scroll bytes out of plain shells. Returns
    /// whether the report was sent, so the caller can pair a forwarded press
    /// with its eventual release.
    pub fn click_pane(
        &mut self,
        id: PaneId,
        button: MouseButton,
        press: bool,
        col: u16,
        row: u16,
    ) -> bool {
        let Some(emulator) = self.emulators.get(&id) else {
            return false;
        };
        if !emulator.wants_mouse_buttons() {
            return false;
        }
        let payload = encode_button(button, press, col, row);
        self.write_pty(id, &payload);
        true
    }

    /// Write straight to a pane's PTY. Bypasses `send_input` on purpose:
    /// input we synthesized on the user's behalf must not clear their scroll
    /// position or land in the prompt log, for the same reason the emulator's
    /// query replies in `poll` bypass it.
    fn write_pty(&mut self, id: PaneId, data: &[u8]) {
        if let Some(backend) = &mut self.backend
            && let Err(e) = backend.send_input(id, data)
        {
            tracing::warn!("failed to send synthesized scroll to pane {id}: {e}");
        }
    }

    fn scroll_up(&mut self, id: PaneId, lines: usize) {
        if lines == 0 {
            return;
        }
        let offset = self.scroll.entry(id).or_insert(0);
        *offset = offset.saturating_add(lines);
    }

    fn scroll_down(&mut self, id: PaneId, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(entry) = self.scroll.get_mut(&id) {
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
        self.sync_scroll_pane(id);
    }

    fn sync_scroll_pane(&mut self, id: PaneId) {
        let offset = self.scroll.get(&id).copied().unwrap_or(0);
        let actual = match self.emulators.get_mut(&id) {
            // The emulator clamps the offset to the actual scrollback
            // buffer size internally, so we can pass the full request
            // through and read back what was applied.
            Some(emulator) => emulator.set_scroll_offset(offset),
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

    /// Resize each listed pane's backend PTY and emulator to its own
    /// (rows, cols), skipping a pane whose size didn't change. `layouts`
    /// carries one entry per currently *visible* pane — panes scrolled out of
    /// the split-view window are omitted and keep their `last_content_size`
    /// until they become visible again.
    pub fn resize_visible_panes(&mut self, layouts: &[(PaneId, u16, u16)]) {
        let active_id = self.active_pane_id();
        for &(id, rows, cols) in layouts {
            // Shared minimum-grid clamp: PTY, emulator, and the recorded
            // size must all agree, or the skip-if-unchanged check and the
            // inner program's wrap width drift apart at degenerate layouts.
            let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
            if Some(id) == active_id {
                self.size = (rows, cols);
            }
            if self.last_content_size.get(&id) == Some(&(rows, cols)) {
                continue;
            }
            if let Some(backend) = &mut self.backend {
                backend.resize(id, rows, cols);
            }
            if let Some(emulator) = self.emulators.get_mut(&id) {
                emulator.resize(rows, cols);
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

    /// Drain pending backend events into pane emulators and pane metadata.
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
                    let Some(emulator) = self.emulators.get_mut(&pane) else {
                        continue;
                    };
                    let events = emulator.process(&data);
                    if let Some(title) = events.title
                        && let Some(info) = self.panes.iter_mut().find(|p| p.id == pane)
                    {
                        info.title = title;
                    }
                    // Terminal query responses (DA, DSR, ...) go back to the
                    // program that asked. Bypasses `send_input` on purpose:
                    // an emulator-generated reply must not clear the user's
                    // scroll position or land in the prompt log.
                    if !events.pty_writes.is_empty()
                        && let Some(backend) = &mut self.backend
                        && let Err(e) = backend.send_input(pane, &events.pty_writes)
                    {
                        tracing::warn!("failed to send terminal reply to pane {pane}: {e}");
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

    /// Allocate a new backend pane and matching emulator. `command`, when
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
        let (rows, cols) = crate::runtime::emulator::effective_size(rows, cols);
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        let id = backend.create_pane(rows, cols, command)?;
        self.emulators
            .insert(id, PaneEmulator::new(rows, cols, SCROLLBACK_LINES));
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

    /// Swap the active pane with the pane at `idx`, moving focus so it follows
    /// the active pane to its new slot (`active` becomes `idx`). Returns `true`
    /// when the swap happened, `false` for an out-of-range `idx` or a self-swap
    /// (both benign no-ops). Only the ordered `panes` Vec changes — all
    /// per-pane state (parsers, scroll, sizes, prompt buffers, backend) is keyed
    /// by `PaneId`, so reordering leaves it untouched.
    pub fn swap_active_with(&mut self, idx: usize) -> bool {
        if idx >= self.panes.len() || idx == self.active {
            return false;
        }
        self.panes.swap(self.active, idx);
        self.active = idx;
        self.sync_visible_window();
        true
    }

    /// Screen for a specific pane, independent of which pane is currently
    /// active — the split-view renderer draws every visible pane, not just
    /// the focused one.
    pub fn screen_for_pane(&self, id: PaneId) -> Option<ScreenView<'_>> {
        self.emulators.get(&id).map(PaneEmulator::view)
    }

    pub fn active_screen(&self) -> Option<ScreenView<'_>> {
        let id = self.active_pane_id()?;
        self.screen_for_pane(id)
    }

    fn remove_pane_state(&mut self, id: PaneId) {
        self.emulators.remove(&id);
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
            fullscreen: TerminalFullscreen::Off,
            last_content_size: HashMap::new(),
            visible_start: 0,
            max_visible_normal: MAX_VISIBLE_NORMAL,
            max_visible_fullscreen: MAX_VISIBLE_FULLSCREEN,
            emulators: HashMap::new(),
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

    /// A state with a FakeBackend plus the shared handle for injecting
    /// synthetic backend events into the next `poll` call.
    fn state_with_event_queue() -> (
        TerminalState,
        std::rc::Rc<std::cell::RefCell<Vec<BackendEvent>>>,
    ) {
        let backend = crate::test_util::FakeBackend::default();
        let events = backend.pending_events.clone();
        (TerminalState::new(Some(Box::new(backend)), false), events)
    }

    /// A single 10x40 pane whose program has already emitted `modes`, with
    /// the payloads recorded during setup discarded so a test sees only what
    /// the scroll itself wrote. The pane's centre is therefore column 21,
    /// row 6.
    fn state_with_pane_in_modes(modes: &[u8]) -> (TerminalState, PaneId) {
        let (mut state, events) = state_with_event_queue();
        state.create_pane().unwrap();
        let id = state.panes[0].id;
        state.resize_visible_panes(&[(id, 10, 40)]);
        events.borrow_mut().push(BackendEvent::Output {
            pane: id,
            data: modes.to_vec(),
        });
        state.poll();
        if let Some(backend) = &mut state.backend {
            backend.send_input(id, b"").ok();
        }
        (state, id)
    }

    /// Plain shell output taller than the 10-row test pane, so lines actually
    /// scroll off the top and land in the emulator's scrollback. Without
    /// overflow there is no history and nothing to scroll into.
    fn shell_output_past_one_screen() -> Vec<u8> {
        (0..20).fold(Vec::new(), |mut out, i| {
            out.extend_from_slice(format!("line{i}\r\n").as_bytes());
            out
        })
    }

    /// Payloads written to the PTY after `state_with_pane_in_modes` set up
    /// the pane, i.e. everything past its trailing empty marker payload.
    fn payloads_after_setup(state: &TerminalState) -> Vec<Vec<u8>> {
        let sent = state.fake_backend_sent().unwrap();
        let marker = sent.iter().rposition(|p| p.is_empty()).unwrap();
        sent[marker + 1..].to_vec()
    }

    #[test]
    fn scroll_active_sends_wheel_notches_to_a_mouse_reporting_pane() {
        // Claude Code's startup mode set. Six lines is two wheel notches.
        let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");

        state.scroll_active(true, 6);

        assert_eq!(
            payloads_after_setup(&state),
            vec![b"\x1b[<64;21;6M\x1b[<64;21;6M".to_vec()]
        );
        assert!(
            state.scroll.is_empty(),
            "a wheel-driven pane must not move the emulator's own view"
        );
    }

    #[test]
    fn scroll_active_rounds_a_partial_notch_up() {
        let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

        // One line still has to move the pane; it must not round down to zero
        // notches and silently do nothing.
        state.scroll_active(false, 1);

        assert_eq!(payloads_after_setup(&state), vec![b"\x1b[<65;21;6M".to_vec()]);
    }

    #[test]
    fn scroll_active_sends_arrow_keys_on_the_alternate_screen() {
        let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1049h");

        state.scroll_active(true, 3);

        assert_eq!(payloads_after_setup(&state), vec![b"\x1b[A\x1b[A\x1b[A".to_vec()]);
    }

    #[test]
    fn scroll_active_uses_application_arrow_keys_when_decckm_is_set() {
        let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1049h\x1b[?1h");

        state.scroll_active(false, 2);

        assert_eq!(payloads_after_setup(&state), vec![b"\x1bOB\x1bOB".to_vec()]);
    }

    #[test]
    fn scroll_active_scrolls_the_emulator_for_a_plain_shell() {
        let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());

        state.scroll_active(true, 3);
        state.sync_scroll();

        assert_eq!(state.scroll.get(&id).copied(), Some(3));
        assert!(
            payloads_after_setup(&state).is_empty(),
            "a shell echoes unbound escape sequences into its prompt, so the \
             scrollback branch must write nothing to the PTY"
        );
    }

    #[test]
    fn scroll_active_down_unwinds_the_emulator_offset_for_a_plain_shell() {
        let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());
        state.scroll_active(true, 3);

        state.scroll_active(false, 3);

        assert!(!state.scroll.contains_key(&id));
        assert!(payloads_after_setup(&state).is_empty());
    }

    #[test]
    fn scroll_active_ignores_a_zero_line_request() {
        let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

        state.scroll_active(true, 0);

        assert!(payloads_after_setup(&state).is_empty());
    }

    #[test]
    fn scroll_pane_moves_a_non_active_panes_view_immediately() {
        let (mut state, events) = state_with_event_queue();
        state.create_pane().unwrap();
        state.create_pane().unwrap();
        let first = state.panes[0].id;
        state.resize_visible_panes(&[(first, 10, 40)]);
        events.borrow_mut().push(BackendEvent::Output {
            pane: first,
            data: shell_output_past_one_screen(),
        });
        state.poll();
        assert_ne!(
            state.active_pane_id(),
            Some(first),
            "test needs the scrolled pane to be non-active"
        );

        state.scroll_pane(first, true, 3, None);

        assert_eq!(state.scroll.get(&first).copied(), Some(3));
        assert_eq!(
            state.emulators.get(&first).unwrap().scroll_offset(),
            3,
            "the per-frame sync only reaches the active pane, so scroll_pane \
             must apply the offset itself"
        );
    }

    #[test]
    fn click_pane_forwards_sgr_press_and_release_to_a_mouse_reporting_pane() {
        let (mut state, id) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");

        assert!(state.click_pane(id, MouseButton::Left, true, 5, 3));
        assert!(state.click_pane(id, MouseButton::Left, false, 5, 3));

        assert_eq!(
            payloads_after_setup(&state),
            vec![b"\x1b[<0;5;3M".to_vec(), b"\x1b[<0;5;3m".to_vec()]
        );
    }

    #[test]
    fn click_pane_stays_silent_for_a_pane_that_never_claimed_the_mouse() {
        let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());

        assert!(!state.click_pane(id, MouseButton::Left, true, 5, 3));
        assert!(!state.click_pane(id, MouseButton::Right, false, 5, 3));

        assert!(
            payloads_after_setup(&state).is_empty(),
            "a shell echoes unbound escape sequences into its prompt, so an \
             unclaimed click must write nothing to the PTY"
        );
    }

    #[test]
    fn wheel_horizontal_pane_forwards_only_to_a_wheel_reporting_pane() {
        let (mut state, id) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

        state.wheel_horizontal_pane(id, true, 5, 2);
        state.wheel_horizontal_pane(id, false, 5, 2);

        assert_eq!(
            payloads_after_setup(&state),
            vec![b"\x1b[<66;5;2M".to_vec(), b"\x1b[<67;5;2M".to_vec()]
        );
    }

    #[test]
    fn wheel_horizontal_pane_stays_silent_for_a_plain_shell() {
        let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());

        state.wheel_horizontal_pane(id, true, 5, 2);

        assert!(
            payloads_after_setup(&state).is_empty(),
            "horizontal wheel has no scrollback fallback, so an unclaimed \
             notch must write nothing to the PTY"
        );
    }

    #[test]
    fn scroll_pane_reports_the_pointer_cell_when_given_one() {
        let (mut state, id) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

        state.scroll_pane(id, true, 3, Some((5, 2)));

        assert_eq!(payloads_after_setup(&state), vec![b"\x1b[<64;5;2M".to_vec()]);
    }

    #[test]
    fn poll_applies_osc_title_to_pane() {
        let (mut state, events) = state_with_event_queue();
        state.create_pane().unwrap();
        let id = state.panes[0].id;

        events.borrow_mut().push(BackendEvent::Output {
            pane: id,
            data: b"\x1b]2;claude\x07".to_vec(),
        });
        state.poll();

        assert_eq!(state.panes[0].title, "claude");
    }

    #[test]
    fn poll_keeps_title_when_output_sets_none() {
        let (mut state, events) = state_with_event_queue();
        state.create_pane_with(None, Some("shell")).unwrap();
        let id = state.panes[0].id;

        events.borrow_mut().push(BackendEvent::Output {
            pane: id,
            data: b"plain output\x1b]2;\x07".to_vec(),
        });
        state.poll();

        // Plain output (and an empty OSC title) must not clobber the label.
        assert_eq!(state.panes[0].title, "shell");
    }

    #[test]
    fn poll_forwards_terminal_query_reply_to_pty() {
        let (mut state, events) = state_with_event_queue();
        state.create_pane().unwrap();
        let id = state.panes[0].id;

        // DSR 6 — the program asks for the cursor position; the emulator's
        // reply must reach the backend PTY.
        events.borrow_mut().push(BackendEvent::Output {
            pane: id,
            data: b"\x1b[6n".to_vec(),
        });
        state.poll();

        let sent = state.fake_backend_sent().unwrap();
        assert_eq!(sent, vec![b"\x1b[1;1R".to_vec()]);
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
    fn later_title_replaces_earlier_within_one_poll() {
        let (mut state, events) = state_with_event_queue();
        state.create_pane().unwrap();
        let id = state.panes[0].id;

        events.borrow_mut().push(BackendEvent::Output {
            pane: id,
            data: b"\x1b]2;first\x07\x1b]2;second\x07".to_vec(),
        });
        state.poll();

        assert_eq!(state.panes[0].title, "second");
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
    fn swap_active_with_exchanges_panes_and_follows_focus() {
        let mut state = state_with_fake();
        state.create_pane_with(None, Some("A")).unwrap();
        state.create_pane_with(None, Some("B")).unwrap();
        state.create_pane_with(None, Some("C")).unwrap();
        state.active = 0; // focus pane "A"
        let a_id = state.panes[0].id;
        let c_id = state.panes[2].id;

        assert!(state.swap_active_with(2));

        // "A" and "C" exchanged slots; focus followed "A" to slot 2.
        assert_eq!(state.panes[0].id, c_id);
        assert_eq!(state.panes[2].id, a_id);
        assert_eq!(state.panes[0].title, "C");
        assert_eq!(state.panes[2].title, "A");
        assert_eq!(state.active, 2);
    }

    #[test]
    fn swap_active_with_out_of_range_is_noop() {
        let mut state = state_with_fake();
        state.create_pane_with(None, Some("A")).unwrap();
        state.create_pane_with(None, Some("B")).unwrap();
        state.active = 0;

        assert!(!state.swap_active_with(5));
        assert_eq!(state.active, 0);
        assert_eq!(state.panes[0].title, "A");
        assert_eq!(state.panes[1].title, "B");
    }

    #[test]
    fn swap_active_with_self_is_noop() {
        let mut state = state_with_fake();
        state.create_pane_with(None, Some("A")).unwrap();
        state.create_pane_with(None, Some("B")).unwrap();
        state.active = 1;

        assert!(!state.swap_active_with(1));
        assert_eq!(state.active, 1);
        assert_eq!(state.panes[1].title, "B");
    }

    #[test]
    fn swap_active_with_preserves_per_pane_state() {
        let mut state = state_with_fake();
        state.create_pane_with(None, Some("A")).unwrap();
        state.create_pane_with(None, Some("B")).unwrap();
        state.active = 0;
        let a_id = state.panes[0].id;
        // Seed scroll/size state keyed by the moving pane's id.
        state.scroll.insert(a_id, 7);
        state.last_content_size.insert(a_id, (10, 40));

        assert!(state.swap_active_with(1));

        // Per-pane state is id-keyed, so it survives the reorder unchanged.
        assert_eq!(state.scroll.get(&a_id), Some(&7));
        assert_eq!(state.last_content_size.get(&a_id), Some(&(10, 40)));
        assert_eq!(state.panes[1].id, a_id);
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
    fn resize_visible_panes_clamps_zero_to_minimum_grid() {
        let mut state = state_with_fake();
        state.create_pane().unwrap();
        let id = state.panes[0].id;

        state.resize_visible_panes(&[(id, 0, 0)]);

        // The recorded size must match the emulator's minimum grid (1x2),
        // not a raw 1x1 clamp — PTY, emulator, and bookkeeping stay in sync.
        assert_eq!(state.last_content_size.get(&id), Some(&(1, 2)));
        assert_eq!(state.screen_for_pane(id).unwrap().size(), (1, 2));
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
        state.fullscreen = TerminalFullscreen::Grid;
        assert_eq!(state.max_visible(), 7);
        state.fullscreen = TerminalFullscreen::Zoom;
        assert_eq!(state.max_visible(), 1);
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
