use super::EventProxy;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// Read-only query surface over an emulated screen. Rows/columns are
/// viewport coordinates: `(0, 0)` is the top-left of what should be drawn,
/// already accounting for the current scrollback offset.
pub struct ScreenView<'a> {
    pub(super) term: &'a Term<EventProxy>,
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
pub(super) fn to_ratatui_color(color: Color) -> ratatui::style::Color {
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