use crate::backend::PaneId;
use crate::input::{encode_arrow, encode_button, encode_wheel, encode_wheel_horizontal};
use crate::runtime::emulator::ScrollSink;
use crossterm::event::MouseButton;

use super::{TerminalState, WHEEL_LINES_PER_NOTCH};

impl TerminalState {
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
    pub(super) fn write_pty(&mut self, id: PaneId, data: &[u8]) {
        if let Some(backend) = &mut self.backend
            && let Err(e) = backend.send_input(id, data)
        {
            tracing::warn!("failed to send synthesized scroll to pane {id}: {e}");
        }
    }

    pub(super) fn scroll_up(&mut self, id: PaneId, lines: usize) {
        if lines == 0 {
            return;
        }
        let offset = self.scroll.entry(id).or_insert(0);
        *offset = offset.saturating_add(lines);
    }

    pub(super) fn scroll_down(&mut self, id: PaneId, lines: usize) {
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

    pub fn sync_scroll(&mut self) {
        let Some(id) = self.active_pane_id() else {
            return;
        };
        self.sync_scroll_pane(id);
    }

    pub(super) fn sync_scroll_pane(&mut self, id: PaneId) {
        let offset = self.scroll.get(&id).copied().unwrap_or(0);
        let actual = match self.emulators.get_mut(&id) {
            // The emulator clamps internally, so pass the full request through
            // and read back what was applied.
            Some(emulator) => emulator.set_scroll_offset(offset),
            None => return,
        };
        if actual == 0 {
            self.scroll.remove(&id);
        } else {
            self.scroll.insert(id, actual);
        }
    }
}
