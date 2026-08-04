//! Turning an emulated screen back into the bytes that reproduce it.
//!
//! A pane's byte ring is history, not a screen. For a program drawing on the
//! alternate screen the recorded bytes are cell updates against a screen the
//! reader does not have, so replaying them paints fragments — but the emulator
//! the hub already runs to follow the pane's modes is holding the screen those
//! updates produced. This turns its grid back into the bytes that paint it.
//!
//! Written as an **absolute repaint**: the screen is cleared with default
//! attributes, every row is positioned by `CUP`, and each run of equal
//! attributes costs one `SGR` that begins with a reset. Nothing in the output
//! depends on where the receiving terminal's cursor was or which attributes it
//! had, so the same snapshot is correct for a client that has just opened a
//! blank terminal and for one being repainted.
//!
//! **What a snapshot does not carry.** Wrap bookkeeping: `WRAPLINE` on a row
//! that continued into the next, and `LEADING_WIDE_CHAR_SPACER` on the filler
//! left when a wide character did not fit the last column. Both describe how a
//! row came to look this way rather than how it looks, and an absolute repaint
//! places each row independently — so a row that wrapped arrives as two rows and
//! a later resize reflows it differently from the original. Nothing reads that
//! difference today: alternate-screen programs redraw on resize, and a
//! normal-screen pane is replayed from its byte ring, which keeps wrapping
//! intact. Underline colour, hyperlinks (OSC 8) and the scrolling region
//! (DECSTBM) are not carried either.

use super::EventProxy;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor};

/// How a cell looks. The remaining flags describe the grid's own bookkeeping
/// (wide-char spacers, wrap continuation) rather than anything a terminal can be
/// told to enter, so they are masked out — and comparing what is left is what
/// lets a run of equal attributes cost one escape.
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
    let mut out = String::from("\x1b[m\x1b[2J");
    // Carried across rows: `SGR` survives a `CUP`, so a run of equal attributes
    // spanning a row boundary still costs one escape.
    let mut pen: Option<Pen> = None;

    for row in 0..rows {
        // Positioned rather than reached by a newline. Writing the last column
        // of a row leaves the cursor pending-wrap, and this `CUP` is what
        // cancels it — which is also why a full row of cells can never scroll
        // the screen. Grid line 0 is the top of the live screen whatever the
        // display offset is, so this does not depend on the emulator's scroll.
        out.push_str(&format!("\x1b[{};1H", row + 1));
        for col in 0..cols {
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
                out.push_str(&sgr(cell_pen));
                pen = Some(cell_pen);
            }
            out.push(cell.c);
            if let Some(zerowidth) = cell.zerowidth() {
                out.extend(zerowidth);
            }
        }
    }

    // The pen the program left set, so the next thing it writes looks the way it
    // means to rather than inheriting the last cell of the screen.
    out.push_str(&sgr(Pen::of(&grid.cursor.template)));
    let point = grid.cursor.point;
    out.push_str(&format!(
        "\x1b[{};{}H",
        point.line.0.max(0) + 1,
        point.column.0 + 1
    ));
    out.into_bytes()
}

/// One absolute `SGR`. Leads with `0` so the sequence states the whole pen
/// rather than a change from whatever the reader had.
fn sgr(pen: Pen) -> String {
    let mut params = vec!["0".to_string()];
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
            params.push(param.to_string());
        }
    }
    if let Some(param) = color_param(pen.fg, true) {
        params.push(param);
    }
    if let Some(param) = color_param(pen.bg, false) {
        params.push(param);
    }
    format!("\x1b[{}m", params.join(";"))
}

/// The `SGR` parameter selecting `color`, or `None` when the default is right —
/// every sequence starts from a reset, so the default needs saying nothing.
///
/// A named colour with no fixed palette slot (`Cursor`, `BrightForeground`,
/// `DimForeground`) defers to the default for the same reason
/// [`to_ratatui_color`](super::view::to_ratatui_color) renders it as `Reset`:
/// where it lands is the host terminal's business, not this snapshot's.
fn color_param(color: Color, foreground: bool) -> Option<String> {
    let (basic, bright, extended) = if foreground {
        (30, 90, 38)
    } else {
        (40, 100, 48)
    };
    match color {
        Color::Spec(rgb) => Some(format!("{extended};2;{};{};{}", rgb.r, rgb.g, rgb.b)),
        Color::Indexed(index) => Some(indexed_param(index, basic, bright, extended)),
        Color::Named(named) => {
            let index = named as usize;
            if index < 16 {
                Some(indexed_param(index as u8, basic, bright, extended))
            } else {
                match named {
                    NamedColor::DimBlack => Some(indexed_param(0, basic, bright, extended)),
                    NamedColor::DimRed => Some(indexed_param(1, basic, bright, extended)),
                    NamedColor::DimGreen => Some(indexed_param(2, basic, bright, extended)),
                    NamedColor::DimYellow => Some(indexed_param(3, basic, bright, extended)),
                    NamedColor::DimBlue => Some(indexed_param(4, basic, bright, extended)),
                    NamedColor::DimMagenta => Some(indexed_param(5, basic, bright, extended)),
                    NamedColor::DimCyan => Some(indexed_param(6, basic, bright, extended)),
                    NamedColor::DimWhite => Some(indexed_param(7, basic, bright, extended)),
                    _ => None,
                }
            }
        }
    }
}

fn indexed_param(index: u8, basic: u8, bright: u8, extended: u8) -> String {
    if index < 8 {
        format!("{}", basic + index)
    } else if index < 16 {
        format!("{}", bright + (index - 8))
    } else {
        format!("{extended};5;{index}")
    }
}
