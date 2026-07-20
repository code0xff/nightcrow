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
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, MIN_COLUMNS, MIN_SCREEN_LINES, Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor};

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
struct EventProxy(Rc<RefCell<ProxyState>>);

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

/// Read-only query surface over an emulated screen. Rows/columns are
/// viewport coordinates: `(0, 0)` is the top-left of what should be drawn,
/// already accounting for the current scrollback offset.
pub struct ScreenView<'a> {
    term: &'a Term<EventProxy>,
}

impl<'a> ScreenView<'a> {
    /// (rows, cols) of the emulated screen.
    pub fn size(&self) -> (u16, u16) {
        let grid = self.term.grid();
        (grid.screen_lines() as u16, grid.columns() as u16)
    }

    /// Cell at a viewport position, or `None` when out of bounds.
    pub fn cell(&self, row: u16, col: u16) -> Option<CellView<'a>> {
        let grid = self.term.grid();
        if usize::from(row) >= grid.screen_lines() || usize::from(col) >= grid.columns() {
            return None;
        }
        // Viewport row 0 maps to grid line -display_offset: scrolling back
        // shifts the view into negative (history) line indices.
        let line = Line(i32::from(row) - grid.display_offset() as i32);
        let point = Point::new(line, Column(usize::from(col)));
        Some(CellView { cell: &grid[point] })
    }

    /// Live cursor position as (row, col), independent of the scrollback
    /// offset and of the program's DECTCEM show/hide state — nightcrow
    /// always exposes the input point of the focused pane.
    pub fn cursor_position(&self) -> (u16, u16) {
        let point = self.term.grid().cursor.point;
        (point.line.0.max(0) as u16, point.column.0 as u16)
    }

    /// Whether the running program enabled bracketed paste (DECSET 2004).
    pub fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }
}

/// One screen cell. Wide characters occupy two columns: the glyph lives on
/// the first cell and the second reports `is_wide_spacer()`.
pub struct CellView<'a> {
    cell: &'a Cell,
}

impl CellView<'_> {
    /// Whether this cell is the spacer half of a wide character (or the
    /// filler before a wide character that wrapped). Emitting text for it
    /// would shift the rest of the row by one column.
    pub fn is_wide_spacer(&self) -> bool {
        self.cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
    }

    /// Append the cell's visible contents (base char plus any zero-width
    /// combining chars) to `out`.
    pub fn append_contents(&self, out: &mut String) {
        out.push(self.cell.c);
        if let Some(zerowidth) = self.cell.zerowidth() {
            out.extend(zerowidth);
        }
    }

    pub fn fg(&self) -> ratatui::style::Color {
        to_ratatui_color(self.cell.fg)
    }

    pub fn bg(&self) -> ratatui::style::Color {
        to_ratatui_color(self.cell.bg)
    }

    pub fn bold(&self) -> bool {
        self.cell.flags.contains(Flags::BOLD)
    }

    pub fn italic(&self) -> bool {
        self.cell.flags.contains(Flags::ITALIC)
    }

    pub fn underline(&self) -> bool {
        self.cell.flags.contains(Flags::UNDERLINE)
    }

    pub fn inverse(&self) -> bool {
        self.cell.flags.contains(Flags::INVERSE)
    }

    pub fn dim(&self) -> bool {
        self.cell.flags.contains(Flags::DIM)
    }
}

/// Map an emulator color to a ratatui color. Named standard/bright colors
/// become the equivalent indexed color so the user's terminal palette
/// applies; default foreground/background become `Reset` for the same
/// reason. Dim named colors map to their base color — `CellView::dim`
/// carries the dim attribute separately.
fn to_ratatui_color(color: Color) -> ratatui::style::Color {
    use ratatui::style::Color as C;
    match color {
        Color::Spec(rgb) => C::Rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => C::Indexed(i),
        Color::Named(named) => {
            let idx = named as usize;
            if idx < 16 {
                C::Indexed(idx as u8)
            } else {
                match named {
                    NamedColor::DimBlack => C::Indexed(0),
                    NamedColor::DimRed => C::Indexed(1),
                    NamedColor::DimGreen => C::Indexed(2),
                    NamedColor::DimYellow => C::Indexed(3),
                    NamedColor::DimBlue => C::Indexed(4),
                    NamedColor::DimMagenta => C::Indexed(5),
                    NamedColor::DimCyan => C::Indexed(6),
                    NamedColor::DimWhite => C::Indexed(7),
                    // Foreground, Background, Cursor, BrightForeground,
                    // DimForeground: no fixed palette slot — defer to the
                    // host terminal's defaults.
                    _ => C::Reset,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contents(view: &ScreenView<'_>, row: u16, col: u16) -> String {
        let mut out = String::new();
        view.cell(row, col).unwrap().append_contents(&mut out);
        out
    }

    #[test]
    fn process_writes_text_and_tracks_cursor() {
        let mut emu = PaneEmulator::new(3, 10, 0);
        emu.process(b"hi");

        assert_eq!(contents(&emu.view(), 0, 0), "h");
        assert_eq!(contents(&emu.view(), 0, 1), "i");
        assert_eq!(emu.view().cursor_position(), (0, 2));
    }

    #[test]
    fn osc_title_is_captured_and_cleaned() {
        let mut emu = PaneEmulator::new(3, 20, 0);
        // Embedded control bytes and surrounding whitespace must not leak
        // into the tab label; OSC 0 (icon + title) works like OSC 2.
        let events = emu.process(b"\x1b]0;  cargo\t test  \x07");
        assert_eq!(events.title.as_deref(), Some("cargo test"));

        // An empty (or whitespace-only) title must not override the current one.
        let events = emu.process(b"\x1b]2;   \x1b\\");
        assert_eq!(events.title, None);
    }

    #[test]
    fn title_is_none_when_chunk_sets_no_title() {
        let mut emu = PaneEmulator::new(3, 20, 0);
        let events = emu.process(b"plain output");
        assert_eq!(events.title, None);
    }

    #[test]
    fn cursor_position_report_produces_pty_write() {
        let mut emu = PaneEmulator::new(5, 20, 0);
        // DSR 6: the program asks where the cursor is; the emulator must
        // answer through the PTY (vt100 silently dropped such queries).
        let events = emu.process(b"\x1b[2;3H\x1b[6n");
        assert_eq!(events.pty_writes, b"\x1b[2;3R");
    }

    #[test]
    fn bracketed_paste_mode_follows_decset() {
        let mut emu = PaneEmulator::new(3, 10, 0);
        assert!(!emu.view().bracketed_paste());
        emu.process(b"\x1b[?2004h");
        assert!(emu.view().bracketed_paste());
        emu.process(b"\x1b[?2004l");
        assert!(!emu.view().bracketed_paste());
    }

    #[test]
    fn wide_char_occupies_two_columns_with_spacer() {
        let mut emu = PaneEmulator::new(3, 10, 0);
        emu.process("가".as_bytes());

        let view = emu.view();
        assert_eq!(contents(&view, 0, 0), "가");
        assert!(!view.cell(0, 0).unwrap().is_wide_spacer());
        assert!(view.cell(0, 1).unwrap().is_wide_spacer());
    }

    #[test]
    fn shrink_through_wide_char_then_erase_does_not_panic() {
        // Regression for the crash that motivated this module: vt100
        // panicked (row.rs clear_wide index out of bounds) when a resize
        // truncated a wide character at the last column and the program
        // then issued Erase Display. See vt100-rust issue #28.
        let mut emu = PaneEmulator::new(5, 20, 0);
        emu.process("가나다라마바사아자차".as_bytes());
        emu.resize(5, 19);
        emu.process(b"\x1b[1;1H\x1b[J");

        // Survival is the contract; the screen must also report the new size.
        assert_eq!(emu.view().size(), (5, 19));
    }

    #[test]
    fn every_shrink_width_survives_wide_char_erase() {
        for cols in 1..20u16 {
            let mut emu = PaneEmulator::new(5, 20, 0);
            emu.process("가나다라마바사아자차".as_bytes());
            emu.resize(5, cols);
            emu.process(b"\x1b[1;1H\x1b[J");
        }
    }

    #[test]
    fn scroll_offset_is_clamped_to_history() {
        let mut emu = PaneEmulator::new(3, 10, 5);
        // 3-row screen + 8 lines written = 5 lines of history (within cap).
        for i in 0..8 {
            emu.process(format!("line{i}\r\n").as_bytes());
        }
        let applied = emu.set_scroll_offset(9999);
        assert_eq!(applied, 5);
        assert_eq!(emu.set_scroll_offset(0), 0);
    }

    #[test]
    fn scrolled_view_shows_history_lines() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        for i in 0..10 {
            emu.process(format!("line{i}\r\n").as_bytes());
        }
        emu.set_scroll_offset(2);
        // Live top row would be line8; scrolled back 2 it must show line6.
        let view = emu.view();
        let row: String = (0..5).map(|c| contents(&view, 0, c)).collect();
        assert_eq!(row, "line6");
    }

    #[test]
    fn scroll_sink_defaults_to_scrollback_for_a_plain_shell() {
        let emu = PaneEmulator::new(3, 10, 100);
        assert_eq!(emu.scroll_sink(), ScrollSink::Scrollback);
    }

    #[test]
    fn scroll_sink_is_mouse_wheel_when_program_reports_sgr_mouse() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        // The exact mode set Claude Code emits on startup.
        emu.process(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h");
        assert_eq!(emu.scroll_sink(), ScrollSink::MouseWheel);
    }

    #[test]
    fn scroll_sink_ignores_mouse_reporting_without_sgr_encoding() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        // X10-encoded mouse reporting: we have no encoder for it, so the
        // pane must not be handed wheel bytes it cannot parse.
        emu.process(b"\x1b[?1000h");
        assert_eq!(emu.scroll_sink(), ScrollSink::Scrollback);
    }

    #[test]
    fn wants_mouse_buttons_when_program_reports_sgr_mouse() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        // The exact mode set Claude Code emits on startup.
        emu.process(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h");
        assert!(emu.wants_mouse_buttons());
    }

    #[test]
    fn wants_mouse_buttons_rejects_shell_and_x10_only_panes() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        // A plain shell never claims the mouse: no click byte may reach it.
        assert!(!emu.wants_mouse_buttons());
        // X10-encoded reporting without SGR: we have no encoder for it.
        emu.process(b"\x1b[?1000h");
        assert!(!emu.wants_mouse_buttons());
    }

    #[test]
    fn scroll_sink_is_arrow_keys_on_alternate_screen() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        emu.process(b"\x1b[?1049h");
        assert_eq!(emu.scroll_sink(), ScrollSink::ArrowKeys);
    }

    #[test]
    fn scroll_sink_falls_back_when_alternate_scroll_is_disabled() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        emu.process(b"\x1b[?1049h\x1b[?1007l");
        assert_eq!(emu.scroll_sink(), ScrollSink::Scrollback);
    }

    #[test]
    fn scroll_sink_prefers_mouse_wheel_over_alternate_screen() {
        let mut emu = PaneEmulator::new(3, 10, 100);
        emu.process(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
        assert_eq!(emu.scroll_sink(), ScrollSink::MouseWheel);
    }

    #[test]
    fn alternate_screen_keeps_no_scrollback() {
        // The reason `ScrollSink` exists: alacritty gives the alternate grid
        // zero history, so a scroll offset there can never leave 0 and the
        // grid has nothing to reveal.
        let mut emu = PaneEmulator::new(3, 10, 100);
        emu.process(b"\x1b[?1049h");
        for i in 0..20 {
            emu.process(format!("line{i}\r\n").as_bytes());
        }
        assert_eq!(emu.set_scroll_offset(999), 0);
    }

    #[test]
    fn app_cursor_follows_decckm() {
        let mut emu = PaneEmulator::new(3, 10, 0);
        assert!(!emu.app_cursor());
        emu.process(b"\x1b[?1h");
        assert!(emu.app_cursor());
        emu.process(b"\x1b[?1l");
        assert!(!emu.app_cursor());
    }

    #[test]
    fn zero_size_is_clamped_to_minimum_grid() {
        // alacritty's documented minimum is 1 line x 2 columns; anything
        // smaller (a 1-column grid especially) breaks wide-char reflow.
        let mut emu = PaneEmulator::new(0, 0, 0);
        assert_eq!(emu.view().size(), (1, 2));
        emu.resize(0, 0);
        assert_eq!(emu.view().size(), (1, 2));
        emu.process(b"x"); // must not panic on the minimal grid
    }

    #[test]
    fn named_colors_map_to_indexed_and_defaults_to_reset() {
        use ratatui::style::Color as C;
        assert_eq!(
            to_ratatui_color(Color::Named(NamedColor::Red)),
            C::Indexed(1)
        );
        assert_eq!(
            to_ratatui_color(Color::Named(NamedColor::BrightWhite)),
            C::Indexed(15)
        );
        assert_eq!(
            to_ratatui_color(Color::Named(NamedColor::DimBlue)),
            C::Indexed(4)
        );
        assert_eq!(
            to_ratatui_color(Color::Named(NamedColor::Foreground)),
            C::Reset
        );
        assert_eq!(to_ratatui_color(Color::Indexed(42)), C::Indexed(42));
        assert_eq!(
            to_ratatui_color(Color::Spec(alacritty_terminal::vte::ansi::Rgb {
                r: 1,
                g: 2,
                b: 3
            })),
            C::Rgb(1, 2, 3)
        );
    }
}
