//! That the pen a snapshot compares blank cells against is the one the old
//! per-cell computation built.
//!
//! [`screen_snapshot`](super::screen_snapshot) takes that pen once and compares
//! every tested cell to it, instead of rebuilding it per cell. The comparison
//! decides which trailing cells are erased, so if the hoisted value differed in
//! any attribute the snapshot would either erase a cell it used to spell out or
//! spell out cells it used to erase.

use super::super::PaneEmulator;
use super::{Pen, is_blank};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Cell;

const ROWS: u16 = 8;
const COLS: u16 = 24;

/// The expression the per-cell tests used before the default pen was hoisted.
fn tested_against_a_fresh_default(cell: &Cell) -> bool {
    cell.c == ' ' && cell.zerowidth().is_none() && Pen::of(cell) == Pen::of(&Cell::default())
}

/// Hold every cell of `emulator`'s screen to the expression the hoist replaced.
fn assert_classified_like_a_fresh_default(emulator: &PaneEmulator) {
    let grid = emulator.term.grid();
    let blank = Pen::of(&Cell::default());
    for row in 0..grid.screen_lines() {
        for col in 0..grid.columns() {
            let cell = &grid[Point::new(Line(row as i32), Column(col))];
            assert_eq!(
                is_blank(cell, &blank),
                tested_against_a_fresh_default(cell),
                "row {row} column {col} is classified differently"
            );
        }
    }
}

/// Everything that distinguishes a cell on the screen: text, attributes,
/// hyperlinks, wide-character halves, and combining marks. Trailing spaces of
/// each kind are what the reverse scan tests; the rest keeps the grid from being
/// only blanks.
fn fixture() -> PaneEmulator {
    let mut emulator = PaneEmulator::new(ROWS, COLS, 0);
    emulator.process(
        concat!(
            "\x1b[?1049h\x1b[2J",
            "\x1b[1;1Hdefault",
            "\x1b[2;1H\x1b[41m   ",
            "\x1b[3;1H\x1b[4m   ",
            "\x1b[4;1H\x1b[7m   ",
            "\x1b[5;1H\x1b]8;;https://example.test/blank\x1b\\   \x1b]8;;\x1b\\",
            "\x1b[6;1He\u{0301}",
            "\x1b[7;1H\u{d55c}\u{ae00}",
            "\x1b[8;1H\x1b[38;5;208m\x1b[48;2;1;2;3m  ",
        )
        .as_bytes(),
    );
    emulator
}

#[test]
fn every_cell_is_classified_blank_like_a_fresh_default_pen_would_classify_it() {
    let emulator = fixture();
    assert_classified_like_a_fresh_default(&emulator);

    let grid = emulator.term.grid();
    let blank = Pen::of(&Cell::default());
    let (mut spaces, mut written) = (0, 0);
    for row in 0..grid.screen_lines() {
        for col in 0..grid.columns() {
            let cell = &grid[Point::new(Line(row as i32), Column(col))];
            if cell.c == ' ' {
                spaces += 1;
                written += usize::from(!is_blank(cell, &blank));
            }
        }
    }
    assert!(spaces > 0, "the fixture must have spaces to classify");
    assert!(
        written > 0,
        "and spaces that carry something, or this proves nothing"
    );
}

/// The same question for the shapes the mixed fixture does not reach: every
/// attribute group on its own, both indexed colours, an untouched screen, and a
/// wide character that wrapped, which leaves a leading spacer behind.
#[test]
fn each_kind_of_cell_is_classified_like_a_fresh_default_pen_would_classify_it() {
    let table: &[(u16, u16, &str)] = &[
        (4, 12, "\x1b[41m      "),
        (4, 12, "\x1b[4m      "),
        (4, 12, "\x1b[7m      "),
        (4, 12, "\x1b[8;3;9m      "),
        (4, 12, "\x1b[38;5;208;48;5;236m   "),
        (4, 12, "\x1b[2J"),
        (
            3,
            4,
            "abc\x1b]8;;https://example.test/wrapped\x1b\\\u{d55c}",
        ),
    ];
    for &(rows, cols, paint) in table {
        let mut emulator = PaneEmulator::new(rows, cols, 0);
        emulator.process(paint.as_bytes());
        assert_classified_like_a_fresh_default(&emulator);
    }
}

/// A blank cell's pen is also what an erased row leaves the reader on. Taking
/// the pen once means every erase writes that same value, so the escapes a row
/// after an erase costs are unchanged: none when its cells are blank, an `SGR`
/// when they are not.
#[test]
fn a_row_after_an_erased_one_still_says_what_its_cells_look_like() {
    let mut emulator = PaneEmulator::new(4, 12, 0);
    emulator.process(b"\x1b[1;1H\x1b[41mRED\x1b[2;1H\x1b[3;1H\x1b[4;1H\x1b[44;97mBLUE");
    let snapshot = String::from_utf8(emulator.screen_snapshot()).expect("a snapshot is text");

    // Rows 2 and 3 are empty, so each is erased and leaves the default pen
    // behind: row 3 costs its erase and nothing else.
    let rows: Vec<&str> = snapshot.split("\x1b[3;1H").collect();
    assert_eq!(rows.len(), 2, "the snapshot must position row 3 once");
    assert!(
        rows[1].starts_with(super::ERASE_TO_END_OF_ROW),
        "an empty row after an erased one needs only its erase, got: {:?}",
        rows[1]
    );
    let row4 = snapshot
        .split("\x1b[4;1H")
        .nth(1)
        .expect("the snapshot must position row 4");
    let blue = row4.find("BLUE").expect("the row's text");
    assert!(
        row4[..blue].contains('m'),
        "a coloured cell after an erase must still be told its pen, got: {:?}",
        &row4[..blue]
    );
}
