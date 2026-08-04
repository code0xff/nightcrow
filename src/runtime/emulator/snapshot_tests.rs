//! Round-trip tests for [`screen_snapshot`](super::snapshot::screen_snapshot):
//! feed bytes to one emulator, snapshot it, replay that snapshot into a fresh
//! one, and require the two screens to be identical.
//!
//! Comparing screens rather than asserting on the escape sequences is what makes
//! these tests about the contract — "a client replayed this sees what the pane
//! shows" — instead of about the encoding, which is free to get shorter.

use super::PaneEmulator;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::Color;

const ROWS: u16 = 6;
const COLS: u16 = 20;

/// One cell as everything a snapshot is expected to carry.
type CellState = (String, Color, Color, Flags);

/// Wrap bookkeeping, which a snapshot deliberately does not carry: it positions
/// every row with its own `CUP`, so nothing records that a row continued into the
/// next or that a wide character was pushed off the last column. See
/// [`snapshot`](super::snapshot).
///
/// `WIDE_CHAR` and `WIDE_CHAR_SPACER` are **not** in here — a wide glyph has to
/// land on the same two columns it started on, and comparing those flags is what
/// proves it did.
fn wrap_bookkeeping() -> Flags {
    Flags::WRAPLINE | Flags::LEADING_WIDE_CHAR_SPACER
}

/// Every cell of the live screen, by everything a snapshot is expected to carry.
fn screen(emulator: &PaneEmulator) -> Vec<CellState> {
    let grid = emulator.term.grid();
    let mut out = Vec::with_capacity(grid.screen_lines() * grid.columns());
    for row in 0..grid.screen_lines() {
        for col in 0..grid.columns() {
            let cell = &grid[Point::new(Line(row as i32), Column(col))];
            let mut contents = String::new();
            contents.push(cell.c);
            if let Some(zerowidth) = cell.zerowidth() {
                contents.extend(zerowidth);
            }
            out.push((
                contents,
                cell.fg,
                cell.bg,
                cell.flags.difference(wrap_bookkeeping()),
            ));
        }
    }
    out
}

fn emulator_running(input: &str) -> PaneEmulator {
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    emulator.process(input.as_bytes());
    emulator
}

/// The screen `input` produced, and the screen a client replayed its snapshot
/// ends up with.
fn round_trip(input: &str) -> (PaneEmulator, PaneEmulator) {
    let origin = emulator_running(input);
    let mut replayed = PaneEmulator::new(ROWS, COLS, 0);
    replayed.process(&origin.screen_snapshot());
    (origin, replayed)
}

fn assert_same_screen(input: &str, what: &str) {
    let (origin, replayed) = round_trip(input);
    assert_eq!(screen(&replayed), screen(&origin), "{what}");
    assert_eq!(
        replayed.view().cursor_position(),
        origin.view().cursor_position(),
        "{what}: the cursor must land where the program left it"
    );
}

#[test]
fn plain_text_is_reproduced_cell_for_cell() {
    assert_same_screen("hello\r\nworld", "plain text");
}

#[test]
fn an_untouched_screen_snapshots_to_an_untouched_screen() {
    assert_same_screen("", "a screen nothing has been written to");
}

#[test]
fn colours_survive_a_snapshot() {
    for (input, what) in [
        ("\x1b[31mred\x1b[m plain", "a basic colour"),
        ("\x1b[91mbright\x1b[m", "a bright colour"),
        ("\x1b[38;5;208morange", "a 256-colour index"),
        ("\x1b[38;2;10;20;30mtruecolour", "a 24-bit colour"),
        ("\x1b[41;97mon red", "a background and a foreground"),
        ("\x1b[7mreversed", "reverse video"),
        ("\x1b[48;5;236mdark bg", "a 256-colour background"),
    ] {
        assert_same_screen(input, what);
    }
}

#[test]
fn attributes_survive_a_snapshot() {
    for (input, what) in [
        ("\x1b[1mbold", "bold"),
        ("\x1b[2mdim", "dim"),
        ("\x1b[3mitalic", "italic"),
        ("\x1b[4munderline", "underline"),
        ("\x1b[21mdouble", "a double underline"),
        ("\x1b[4:3mcurly", "an undercurl"),
        ("\x1b[9mstruck", "strikeout"),
        ("\x1b[8mhidden", "hidden text"),
        ("\x1b[1;3;4;7;9mall at once", "several attributes together"),
    ] {
        assert_same_screen(input, what);
    }
}

/// The case the vt100 crate crashed on, and the one where a spacer cell has to be
/// skipped rather than emitted — writing anything for it would shift the rest of
/// the row by a column.
#[test]
fn a_wide_character_keeps_its_columns() {
    assert_same_screen("한글 텍스트", "wide characters");
}

#[test]
fn a_wide_character_wrapping_at_the_last_column_keeps_its_columns() {
    // 19 narrow cells leave one column, which a wide character cannot fit: the
    // grid puts a leading spacer there and wraps the glyph to the next row.
    let input = format!("{}한", "n".repeat(COLS as usize - 1));
    assert_same_screen(&input, "a wide character that wrapped");
}

#[test]
fn a_combining_character_stays_on_its_cell() {
    assert_same_screen("e\u{0301}tude", "a combining accent");
}

/// A snapshot has to be able to paint the bottom-right cell without pushing the
/// screen up a line, which is why every row is positioned rather than reached by
/// a newline.
#[test]
fn a_full_last_row_does_not_scroll_the_screen() {
    let mut input = String::new();
    for row in 0..ROWS {
        input.push_str(&format!("\x1b[{};1H", row + 1));
        input.push_str(&"x".repeat(COLS as usize));
    }
    assert_same_screen(&input, "a screen filled to its last cell");
}

/// What the whole snapshot exists for: an alternate-screen program's paint, which
/// its recorded bytes cannot rebuild.
#[test]
fn an_alternate_screen_paint_is_reproduced() {
    assert_same_screen(
        "\x1b[?1049h\x1b[2J\x1b[3;5HPAINTED\x1b[5;1H\x1b[44m bar \x1b[m",
        "an alternate-screen paint",
    );
}

#[test]
fn a_screen_that_scrolled_snapshots_what_is_on_it() {
    let mut input = String::new();
    // Twice the screen's worth, so the early lines have scrolled off.
    for line in 0..(ROWS * 2) {
        input.push_str(&format!("line {line}\r\n"));
    }
    assert_same_screen(&input, "a screen that has scrolled");
}

#[test]
fn the_cursor_lands_where_the_program_left_it() {
    let (origin, replayed) = round_trip("\x1b[4;7Hx\x1b[2;3H");
    assert_eq!(
        origin.view().cursor_position(),
        (1, 2),
        "the fixture moved it"
    );
    assert_eq!(replayed.view().cursor_position(), (1, 2));
}

/// The pen is part of the screen's state: a program that set a colour and has not
/// written with it yet expects the next thing it writes to carry it.
#[test]
fn the_pen_the_program_left_set_is_restored() {
    let input = "plain\x1b[31;1m";
    let mut origin = emulator_running(input);
    let mut replayed = PaneEmulator::new(ROWS, COLS, 0);
    replayed.process(&origin.screen_snapshot());

    // Written after the snapshot, so it can only look right if the snapshot
    // restored the pen rather than leaving the reader at a bare reset.
    origin.process(b"X");
    replayed.process(b"X");

    assert_eq!(screen(&replayed), screen(&origin));
}

/// A space carrying a colour looks like an empty cell but is not one. Erasing the
/// end of a row is only allowed where what it leaves behind is identical.
#[test]
fn trailing_spaces_that_carry_attributes_are_not_erased() {
    for (input, what) in [
        ("\x1b[41m      ", "trailing spaces with a background"),
        ("\x1b[4m      ", "trailing underlined spaces"),
        ("\x1b[7m      ", "trailing reversed spaces"),
    ] {
        assert_same_screen(input, what);
    }
}

/// What the erasing is for. A large pane is mostly empty most of the time, and
/// spelling every blank cell out is what a replay pays for.
#[test]
fn a_mostly_empty_screen_costs_far_less_than_its_cells() {
    let (rows, cols) = (100u16, 300u16);
    let mut emulator = PaneEmulator::new(rows, cols, 0);
    emulator.process(b"\x1b[?1049h\x1b[2J\x1b[50;10Hjust this");

    let snapshot = emulator.screen_snapshot();
    let cells = usize::from(rows) * usize::from(cols);
    assert!(
        snapshot.len() < cells / 4,
        "a screen of {cells} cells with one line on it must not cost {} bytes",
        snapshot.len()
    );
}
