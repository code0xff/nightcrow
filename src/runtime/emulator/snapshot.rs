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
//! the snapshot stands in only for the screen itself. Underline colour and the
//! scrolling region (DECSTBM) are not carried either. OSC 8 hyperlink runs and
//! the cursor's active hyperlink are carried so a reattaching client can keep
//! links clickable after the repaint.

use super::EventProxy;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags, Hyperlink};
use alacritty_terminal::vte::ansi::Color;
use std::fmt::Write as _;

#[path = "snapshot_write.rs"]
mod write;

pub(in crate::runtime::emulator) use write::write_hyperlink;
use write::{clear_hyperlink, write_sgr};

/// How a cell looks. The remaining flags describe the grid's own bookkeeping
/// rather than anything a terminal can be told to enter, so they are masked
/// out — and comparing what is left is what lets a run of equal attributes
/// cost one escape.
#[derive(PartialEq, Eq, Clone)]
struct Pen {
    fg: Color,
    bg: Color,
    flags: Flags,
    hyperlink: Option<Hyperlink>,
}

impl Pen {
    fn of(cell: &Cell) -> Self {
        Self {
            fg: cell.fg,
            bg: cell.bg,
            flags: cell.flags & rendered_flags(),
            hyperlink: cell.hyperlink(),
        }
    }
}

/// Whether a cell is indistinguishable from one that was never written, so a
/// run of them at the end of a row can be erased instead of spelled out.
///
/// Held to *every* attribute, not just the background: a space carrying a
/// foreground colour looks the same but is not the same cell, and erasing it
/// would hand a client a screen that differs from the one it replaces.
///
/// `blank` is the pen of [`Cell::default`], taken once per snapshot: this runs
/// on every cell of each row's reverse scan, and building the pen again per
/// tested cell cloned its hyperlink just as often.
fn is_blank(cell: &Cell, blank: &Pen) -> bool {
    cell.c == ' ' && cell.zerowidth().is_none() && Pen::of(cell) == *blank
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
    // SGR reset does not clear OSC 8 in alacritty, and a receiver may have
    // inherited a link from bytes just before this snapshot.
    out.push_str(OSC8_CLEAR);
    out.push_str("\x1b[m\x1b[2J");
    // Carried across rows: `SGR` survives a `CUP`, so a run of equal
    // attributes spanning a row boundary still costs one escape.
    let mut pen: Option<Pen> = None;
    let mut hyperlink: Option<Hyperlink> = None;
    // Compared against by `is_blank` and left behind by every erase: the pen a
    // cell that was never written carries.
    let blank_pen = Pen::of(&Cell::default());

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
            .find(|&col| !is_blank(&grid[Point::new(Line(row as i32), Column(col))], &blank_pen));
        let Some(last) = last else {
            clear_hyperlink(&mut out, &mut hyperlink);
            out.push_str(ERASE_TO_END_OF_ROW);
            pen = Some(blank_pen.clone());
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
                // A leading spacer has no glyph in this row to recreate it. A
                // linked one still needs a real write so OSC 8 metadata survives.
                if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
                    && cell.hyperlink().is_some()
                {
                    let cell_pen = Pen::of(cell);
                    if hyperlink != cell_pen.hyperlink {
                        write_hyperlink(&mut out, cell_pen.hyperlink.as_ref());
                        hyperlink = cell_pen.hyperlink.clone();
                    }
                    if pen.as_ref() != Some(&cell_pen) {
                        write_sgr(&mut out, &cell_pen);
                        pen = Some(cell_pen);
                    }
                    out.push(' ');
                }
                continue;
            }
            let cell_pen = Pen::of(cell);
            if hyperlink != cell_pen.hyperlink {
                write_hyperlink(&mut out, cell_pen.hyperlink.as_ref());
                hyperlink = cell_pen.hyperlink.clone();
            }
            if pen.as_ref() != Some(&cell_pen) {
                write_sgr(&mut out, &cell_pen);
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
            clear_hyperlink(&mut out, &mut hyperlink);
            out.push_str(ERASE_TO_END_OF_ROW);
            pen = Some(blank_pen.clone());
        }
    }

    // The pen the program left set, so the next thing it writes looks the way it
    // means to rather than inheriting the last cell of the screen.
    let template_pen = Pen::of(&grid.cursor.template);
    if pen.as_ref() != Some(&template_pen) {
        write_sgr(&mut out, &template_pen);
    }
    if hyperlink != template_pen.hyperlink {
        write_hyperlink(&mut out, template_pen.hyperlink.as_ref());
    }
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
const OSC8_CLEAR: &str = "\x1b]8;;\x1b\\";

#[cfg(test)]
#[path = "snapshot_pen_tests.rs"]
mod pen_tests;

#[cfg(test)]
#[path = "snapshot_pen_cost_tests.rs"]
mod pen_cost_tests;
