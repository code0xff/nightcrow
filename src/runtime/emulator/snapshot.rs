//! Turning an emulated screen back into the bytes that reproduce it.
//!
//! A pane's byte ring is history, not a screen. For an alternate-screen
//! program the recorded bytes are cell updates against a screen the reader
//! does not have; a normal-screen program that repaints in place instead
//! rotates the byte-bounded ring until the bytes that painted the screen are
//! evicted. Either way the emulator already runs is holding the screen those
//! bytes produced, and this turns its grid back into bytes that paint it.
//!
//! Written as an **absolute repaint**: screen cleared, every row positioned by
//! `CUP`, each attribute run costing one reset-leading `SGR`. Nothing depends
//! on where the receiving terminal's cursor was or which attributes it had, so
//! the same snapshot is correct for a fresh terminal and for a repaint.
//!
//! **What a snapshot does not carry.** Wrap bookkeeping (`WRAPLINE`,
//! `LEADING_WIDE_CHAR_SPACER`): both describe how a row came to look this way
//! rather than how it looks, and an absolute repaint places each row
//! independently — so a wrapped row arrives as two rows and a later resize
//! reflows it differently. Nothing reads that difference today:
//! alternate-screen programs redraw on resize, and a normal-screen pane's
//! history is still replayed from its byte ring, which keeps its wrapping —
//! the snapshot stands in only for the screen itself. Underline colour,
//! hyperlinks (OSC 8) and the scrolling region (DECSTBM) are not carried
//! either.

use super::EventProxy;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use std::fmt::Write as _;

/// How a cell looks. The remaining flags describe the grid's own bookkeeping
/// rather than anything a terminal can be told to enter, so they are masked
/// out — and comparing what is left is what lets a run of equal attributes
/// cost one escape.
#[derive(PartialEq, Eq, Clone, Copy)]
struct Pen {
    fg: Color,
    bg: Color,
    flags: Flags,
}

impl Pen {
    fn of(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & rendered_flags(),
        }
    }
}

/// Whether a cell is indistinguishable from one that was never written, so a
/// run of them at the end of a row can be erased instead of spelled out.
///
/// Held to *every* attribute, not just the background: a space carrying a
/// foreground colour looks the same but is not the same cell, and erasing it
/// would hand a client a screen that differs from the one it replaces.
fn is_blank(cell: &Cell) -> bool {
    cell.c == ' ' && cell.zerowidth().is_none() && Pen::of(cell) == Pen::of(&Cell::default())
}

fn rendered_flags() -> Flags {
    Flags::INVERSE
        | Flags::BOLD
        | Flags::ITALIC
        | Flags::DIM
        | Flags::HIDDEN
        | Flags::STRIKEOUT
        | Flags::ALL_UNDERLINES
}

/// The bytes that paint `term`'s current screen, cursor included.
pub(super) fn screen_snapshot(term: &Term<EventProxy>) -> Vec<u8> {
    let grid = term.grid();
    let (rows, cols) = (grid.screen_lines(), grid.columns());
    // One escape and one glyph per cell is the floor; the slack covers each
    // row's `CUP` and the attribute runs. Reserved up front because a
    // snapshot of a large pane is taken on the worker's tick, where a dozen
    // reallocations of a megabyte-long string is the whole cost.
    let mut out = String::with_capacity(rows * cols + rows * 16 + 32);
    out.push_str("\x1b[m\x1b[2J");
    // Carried across rows: `SGR` survives a `CUP`, so a run of equal
    // attributes spanning a row boundary still costs one escape.
    let mut pen: Option<Pen> = None;

    for row in 0..rows {
        // Positioned rather than reached by a newline. Writing the last column
        // of a row leaves the cursor pending-wrap, and this `CUP` cancels it —
        // also why a full row of cells can never scroll the screen. Grid line
        // 0 is the top of the live screen whatever the display offset is.
        let _ = write!(out, "\x1b[{};1H", row + 1);
        // Everything past the last cell worth naming is erased rather than
        // spelled out. Most screens are mostly empty, and this is the
        // difference between a snapshot of a few kilobytes and several
        // hundred.
        let last = (0..cols)
            .rev()
            .find(|&col| !is_blank(&grid[Point::new(Line(row as i32), Column(col))]));
        let Some(last) = last else {
            out.push_str(ERASE_TO_END_OF_ROW);
            pen = Some(Pen::of(&Cell::default()));
            continue;
        };
        for col in 0..=last {
            let cell = &grid[Point::new(Line(row as i32), Column(col))];
            // The second half of a wide character, or the filler left before one
            // that wrapped: the glyph already covers this column, and emitting
            // anything here would shift the rest of the row.
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let cell_pen = Pen::of(cell);
            if pen != Some(cell_pen) {
                write_sgr(&mut out, cell_pen);
                pen = Some(cell_pen);
            }
            out.push(cell.c);
            if let Some(zerowidth) = cell.zerowidth() {
                out.extend(zerowidth);
            }
        }
        // Only when the row was not written to its last column: there the
        // cursor is left pending-wrap *on* that column, and erasing from there
        // would wipe the cell just written.
        if last + 1 < cols {
            out.push_str(ERASE_TO_END_OF_ROW);
            pen = Some(Pen::of(&Cell::default()));
        }
    }

    // The pen the program left set, so the next thing it writes looks the way it
    // means to rather than inheriting the last cell of the screen.
    write_sgr(&mut out, Pen::of(&grid.cursor.template));
    let point = grid.cursor.point;
    let _ = write!(
        out,
        "\x1b[{};{}H",
        point.line.0.max(0) + 1,
        point.column.0 + 1
    );
    out.into_bytes()
}

/// Erase the rest of the row to blank cells. The reset leads because `EL`
/// erases with the *current* background, and what it has to leave behind is
/// the default one — which is what makes the erased cells equal the cells
/// they stand in for.
const ERASE_TO_END_OF_ROW: &str = "\x1b[m\x1b[K";

/// One absolute `SGR`. Leads with `0` so the sequence states the whole pen
/// rather than a change from whatever the reader had.
///
/// Appended in place rather than returned: on a densely coloured screen this
/// runs once per cell, and building a string per call was measurably the cost
/// of the whole snapshot.
fn write_sgr(out: &mut String, pen: Pen) {
    out.push_str("\x1b[0");
    for (flag, param) in [
        (Flags::BOLD, "1"),
        (Flags::DIM, "2"),
        (Flags::ITALIC, "3"),
        (Flags::UNDERLINE, "4"),
        (Flags::DOUBLE_UNDERLINE, "21"),
        (Flags::UNDERCURL, "4:3"),
        (Flags::DOTTED_UNDERLINE, "4:4"),
        (Flags::DASHED_UNDERLINE, "4:5"),
        (Flags::INVERSE, "7"),
        (Flags::HIDDEN, "8"),
        (Flags::STRIKEOUT, "9"),
    ] {
        if pen.flags.contains(flag) {
            out.push(';');
            out.push_str(param);
        }
    }
    write_color(out, pen.fg, true);
    write_color(out, pen.bg, false);
    out.push('m');
}

/// The `SGR` parameter selecting `color`. The default is written as nothing —
/// every sequence starts from a reset, so it needs no saying.
///
/// A named colour with no fixed palette slot (`Cursor`, `BrightForeground`,
/// `DimForeground`) defers to the default for the same reason
/// [`to_ratatui_color`](super::view::to_ratatui_color) renders it as `Reset`:
/// where it lands is the host terminal's business, not this snapshot's.
fn write_color(out: &mut String, color: Color, foreground: bool) {
    let (basic, bright, extended) = if foreground {
        (30, 90, 38)
    } else {
        (40, 100, 48)
    };
    match color {
        Color::Spec(rgb) => {
            let _ = write!(out, ";{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b);
        }
        Color::Indexed(index) => write_indexed(out, index, basic, bright, extended),
        Color::Named(named) => {
            let index = named as usize;
            if index < 16 {
                write_indexed(out, index as u8, basic, bright, extended);
            } else if let Some(dim) = dim_slot(named) {
                write_indexed(out, dim, basic, bright, extended);
            }
        }
    }
}

/// The palette slot a dim named colour stands for. `CellView::dim` carries the dim
/// attribute itself, matching how the renderers read it.
fn dim_slot(named: NamedColor) -> Option<u8> {
    match named {
        NamedColor::DimBlack => Some(0),
        NamedColor::DimRed => Some(1),
        NamedColor::DimGreen => Some(2),
        NamedColor::DimYellow => Some(3),
        NamedColor::DimBlue => Some(4),
        NamedColor::DimMagenta => Some(5),
        NamedColor::DimCyan => Some(6),
        NamedColor::DimWhite => Some(7),
        _ => None,
    }
}

fn write_indexed(out: &mut String, index: u8, basic: u8, bright: u8, extended: u8) {
    if index < 8 {
        let _ = write!(out, ";{}", basic + index);
    } else if index < 16 {
        let _ = write!(out, ";{}", bright + (index - 8));
    } else {
        let _ = write!(out, ";{extended};5;{index}");
    }
}
