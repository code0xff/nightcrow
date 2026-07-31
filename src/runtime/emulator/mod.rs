//! alacritty_terminal-backed per-pane terminal emulator.
//!
//! Wraps `Term` + the ANSI `Processor` + an event proxy behind the narrow
//! contract the rest of nightcrow needs — feed PTY bytes, resize, scroll,
//! query cells/cursor/modes — so no module outside this file touches
//! alacritty internals except through `ScreenView`/`CellView`. This replaced
//! the vt100 crate, whose resize path panicked when a wide character was
//! truncated at the last column (vt100-rust issue #28) and whose upstream
//! had stalled on that class of boundary bugs.

use std::cell::RefCell;
use std::rc::Rc;

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, MIN_COLUMNS, MIN_SCREEN_LINES, Term, TermMode};
use alacritty_terminal::vte::ansi::Processor;

/// Side effects surfaced by `PaneEmulator::process` for the caller to act on.
#[derive(Default)]
pub struct EmulatorEvents {
    /// Most recent OSC 0/2 window title in the processed chunk, already
    /// stripped of control characters and surrounding whitespace. `None`
    /// when the chunk set no (non-empty) title.
    pub title: Option<String>,
    /// Terminal query responses (DA, DSR, ...) the emulator produced while
    /// processing. Must be written back to the pane's PTY so programs that
    /// interrogate their terminal (vim, tmux, ...) receive an answer.
    pub pty_writes: Vec<u8>,
}

/// Where a scroll request for a pane must be delivered. A program that owns
/// its viewport keeps its transcript in its own memory, not in the emulator's
/// scrollback, so scrolling the grid would move nothing; the scroll has to
/// reach the program as input instead. Which input it expects is announced by
/// the modes the program itself enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollSink {
    /// The program tracks the mouse and reports in SGR (1006) form: send it
    /// wheel events. Claude Code lands here.
    MouseWheel,
    /// The program is on the alternate screen and left xterm's
    /// `alternateScroll` (1007) enabled: send it arrow keys. `less`, `man`.
    ArrowKeys,
    /// Nothing claimed the scroll, so the emulator's own scrollback owns it.
    /// Interactive shells land here — the default, and the only branch that
    /// writes nothing to the PTY. A shell would echo an unbound escape
    /// sequence straight into its prompt, so this branch must stay silent.
    Scrollback,
}

#[derive(Default)]
struct ProxyState {
    title: Option<String>,
    pty_writes: Vec<u8>,
}

/// Event sink handed to `Term`. `EventListener::send_event` only gets
/// `&self`, so the collected state lives behind `Rc<RefCell>`; the emulator
/// keeps a second handle and drains it after each `process` call.
#[derive(Clone, Default)]
pub(super) struct EventProxy(Rc<RefCell<ProxyState>>);

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Title(title) => {
                let cleaned: String = title.chars().filter(|c| !c.is_control()).collect();
                let trimmed = cleaned.trim();
                if !trimmed.is_empty() {
                    self.0.borrow_mut().title = Some(trimmed.to_string());
                }
            }
            Event::PtyWrite(text) => {
                self.0
                    .borrow_mut()
                    .pty_writes
                    .extend_from_slice(text.as_bytes());
            }
            // Clipboard, bell, damage and child-process events are not part
            // of nightcrow's pane contract; drop them.
            _ => {}
        }
    }
}

pub struct PaneEmulator {
    term: Term<EventProxy>,
    processor: Processor,
    proxy: EventProxy,
}

impl PaneEmulator {
    /// Create an emulator with a `rows` x `cols` screen (clamped to the
    /// minimum grid alacritty supports) and `scrollback_lines` of history.
    pub fn new(rows: u16, cols: u16, scrollback_lines: usize) -> Self {
        let config = Config {
            scrolling_history: scrollback_lines,
            ..Config::default()
        };
        let proxy = EventProxy::default();
        let term = Term::new(config, &term_size(rows, cols), proxy.clone());
        Self {
            term,
            processor: Processor::new(),
            proxy,
        }
    }

    /// Feed raw PTY output through the emulator, updating the screen state.
    /// Returns the side effects (title change, terminal query responses)
    /// the caller must apply.
    pub fn process(&mut self, bytes: &[u8]) -> EmulatorEvents {
        self.processor.advance(&mut self.term, bytes);
        let mut state = self.proxy.0.borrow_mut();
        EmulatorEvents {
            title: state.title.take(),
            pty_writes: std::mem::take(&mut state.pty_writes),
        }
    }

    /// Resize the emulated screen, reflowing wrapped lines. Safe for any
    /// size change, including one that cuts a wide character at the new
    /// last column (the vt100 panic this module exists to avoid).
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.term.resize(term_size(rows, cols));
    }

    /// Set the absolute scrollback offset (0 = live bottom, larger = older)
    /// and return the offset actually applied after clamping to the
    /// available history.
    pub fn set_scroll_offset(&mut self, offset: usize) -> usize {
        let current = self.term.grid().display_offset();
        let delta = i64::try_from(offset)
            .unwrap_or(i64::MAX)
            .saturating_sub(current as i64)
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
        self.term.scroll_display(Scroll::Delta(delta));
        self.term.grid().display_offset()
    }

    /// Current scrollback offset (0 = live bottom). Production code tracks
    /// the applied offset through `set_scroll_offset`'s return value; this
    /// read-back exists for tests asserting emulator state directly.
    #[cfg(test)]
    pub fn scroll_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Read-only view of the screen as currently scrolled.
    pub fn view(&self) -> ScreenView<'_> {
        ScreenView { term: &self.term }
    }

    /// Which input, if any, a scroll request for this pane must be turned
    /// into. See `ScrollSink`. Mouse reporting wins over `alternateScroll`
    /// because a program that asked for wheel events wants them even on the
    /// alternate screen — that is also the order xterm resolves them in.
    ///
    /// `MOUSE_MODE` alone is not enough: without `SGR_MOUSE` the program
    /// expects the legacy X10 encoding, which cannot address columns past
    /// 223. Rather than emit a second encoding for a case no modern TUI
    /// uses, such a pane falls back to `Scrollback`.
    pub fn scroll_sink(&self) -> ScrollSink {
        let mode = self.term.mode();
        if mode.intersects(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE) {
            ScrollSink::MouseWheel
        } else if mode.contains(TermMode::ALT_SCREEN) && mode.contains(TermMode::ALTERNATE_SCROLL) {
            ScrollSink::ArrowKeys
        } else {
            ScrollSink::Scrollback
        }
    }

    /// Whether the program asked for mouse button reports in SGR form —
    /// the gate for forwarding clicks. The mode set is the same one that
    /// routes wheel scrolls to `ScrollSink::MouseWheel`, but it is a
    /// separate predicate because the meaning differs: a click has no
    /// scrollback fallback, it is either claimed by the program or dropped.
    pub fn wants_mouse_buttons(&self) -> bool {
        let mode = self.term.mode();
        mode.intersects(TermMode::MOUSE_MODE) && mode.contains(TermMode::SGR_MOUSE)
    }

    /// Whether the program enabled DECCKM (application cursor keys), which
    /// changes the arrow-key encoding from `ESC [ A` to `ESC O A`.
    pub fn app_cursor(&self) -> bool {
        self.term.mode().contains(TermMode::APP_CURSOR)
    }

    /// The modes a later-attaching client has to be told about. See
    /// [`PaneModes`].
    pub fn modes(&self) -> PaneModes {
        let mode = self.term.mode();
        PaneModes {
            alt_screen: mode.contains(TermMode::ALT_SCREEN),
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
            show_cursor: mode.contains(TermMode::SHOW_CURSOR),
            line_wrap: mode.contains(TermMode::LINE_WRAP),
            mouse_click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
            mouse_drag: mode.contains(TermMode::MOUSE_DRAG),
            mouse_motion: mode.contains(TermMode::MOUSE_MOTION),
            sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
            utf8_mouse: mode.contains(TermMode::UTF8_MOUSE),
            alternate_scroll: mode.contains(TermMode::ALTERNATE_SCROLL),
            focus_in_out: mode.contains(TermMode::FOCUS_IN_OUT),
        }
    }
}

/// Clamp a requested pane size to alacritty's supported minimum grid.
/// `Term` expects its embedder to enforce `MIN_COLUMNS`/`MIN_SCREEN_LINES`
/// (the alacritty app clamps its window the same way); in particular a
/// 1-column grid makes wide-character reflow loop forever on resize.
///
/// `TerminalState` applies the same clamp to the backend PTY size and its
/// `last_content_size` bookkeeping, so the PTY, the emulator grid, and the
/// recorded size can never diverge at degenerate layouts.
pub fn effective_size(rows: u16, cols: u16) -> (u16, u16) {
    (
        rows.max(MIN_SCREEN_LINES as u16),
        cols.max(MIN_COLUMNS as u16),
    )
}

fn term_size(rows: u16, cols: u16) -> TermSize {
    let (rows, cols) = effective_size(rows, cols);
    TermSize::new(usize::from(cols), usize::from(rows))
}

mod modes;
mod view;

pub use modes::PaneModes;
pub use view::{CellView, ScreenView};

#[cfg(test)]
mod modes_tests;
#[cfg(test)]
mod tests;
