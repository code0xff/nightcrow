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
        // A space carrying a combining mark: the `zerowidth` half of the test.
        (4, 12, " \u{0301}    "),
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

/// What each row of a snapshot spells out once its escape sequences are removed,
/// and whether it ends by erasing the rest of the row.
fn spelled_rows(snapshot: &[u8], rows: u16) -> Vec<(String, bool)> {
    let text = String::from_utf8(snapshot.to_vec()).expect("a snapshot is text");
    (0..rows)
        .map(|row| {
            let start = format!("\x1b[{};1H", row + 1);
            let end = format!("\x1b[{};1H", row + 2);
            let payload = text
                .split(&start)
                .nth(1)
                .map(|rest| rest.split(&end).next().unwrap_or(rest))
                .unwrap_or_default();
            let mut visible = String::new();
            let mut erased = false;
            let mut chars = payload.chars().peekable();
            while let Some(character) = chars.next() {
                if character != '\u{1b}' {
                    visible.push(character);
                    continue;
                }
                match chars.next() {
                    // A control sequence: its final byte is the last one in the
                    // printable range, and `K` is the erase.
                    Some('[') => {
                        let mut last = ' ';
                        for next in chars.by_ref() {
                            last = next;
                            if ('\u{40}'..='\u{7e}').contains(&next) {
                                break;
                            }
                        }
                        erased |= last == 'K';
                    }
                    // An OSC, which runs to `BEL` or `ESC \`.
                    Some(']') => {
                        let mut previous = ' ';
                        for next in chars.by_ref() {
                            if next == '\u{7}' || (previous == '\u{1b}' && next == '\\') {
                                break;
                            }
                            previous = next;
                        }
                    }
                    _ => {}
                }
            }
            (visible, erased)
        })
        .collect()
}

/// The pen the tests above hold the predicate to is only half the contract: the
/// snapshot has to *use* it. Read the bytes it wrote back — the cells it spelled out
/// and where it erased — and require both to follow the default pen rather than
/// whatever pen the emitter happened to build.
#[test]
fn the_snapshot_spells_exactly_the_cells_the_default_pen_calls_written() {
    let (rows, cols) = (4u16, 12usize);
    let mut emulator = PaneEmulator::new(rows, cols as u16, 0);
    emulator.process(
        concat!(
            "\x1b[1;1Hplain",
            "\x1b[2;1H\x1b[41mab  ",
            "\x1b[3;1H\x1b[4m   ",
            "\x1b[4;1H\x1b[38;5;208mx",
        )
        .as_bytes(),
    );
    let snapshot = emulator.screen_snapshot();
    let spelled = spelled_rows(&snapshot, rows);
    let grid = emulator.term.grid();
    let blank = Pen::of(&Cell::default());

    for row in 0..rows {
        let last = (0..cols)
            .rev()
            .find(|&col| !is_blank(&grid[Point::new(Line(row as i32), Column(col))], &blank));
        let (expected, expects_erase) = match last {
            None => (String::new(), true),
            Some(last) => {
                let mut text = String::new();
                for col in 0..=last {
                    let cell = &grid[Point::new(Line(row as i32), Column(col))];
                    text.push(cell.c);
                    if let Some(zerowidth) = cell.zerowidth() {
                        text.extend(zerowidth);
                    }
                }
                (text, last + 1 < cols)
            }
        };
        let (text, erased) = &spelled[row as usize];
        assert_eq!(
            text,
            &expected,
            "row {} must spell the cells the default pen calls written",
            row + 1
        );
        assert_eq!(
            *erased,
            expects_erase,
            "row {} must erase the rest of itself only when nothing follows",
            row + 1
        );
    }
}

/// What the hoist saves at the predicate that carried it: one blank comparison per
/// cell of a row's reverse scan, on the screen a snapshot is most expensive for.
///
/// "As written" is the old source as the compiler saw it, free to lift the default
/// cell out itself; "barriered" pins that construction inside the call, which is
/// what the removal is worth when it cannot be lifted.
///
/// ```text
/// cargo test --release measure_blank_test -- --ignored --nocapture
/// ```
#[test]
#[ignore]
fn measure_blank_test_cost() {
    let mut emulator = PaneEmulator::new(130, 500, 0);
    emulator.process(b"\x1b[?1049h\x1b[2J\x1b[10;1Hsome content");
    let grid = emulator.term.grid();
    let mut cells = Vec::with_capacity(grid.screen_lines() * grid.columns());
    for row in 0..grid.screen_lines() {
        for col in 0..grid.columns() {
            cells.push(&grid[Point::new(Line(row as i32), Column(col))]);
        }
    }
    let blank = Pen::of(&Cell::default());
    let runs = 20u32;
    let calls = (cells.len() as u32) * runs;
    let timing = |classify: &dyn Fn(&Cell) -> bool| {
        let start = std::time::Instant::now();
        let mut classified = 0usize;
        for _ in 0..runs {
            for cell in &cells {
                classified += usize::from(classify(cell));
            }
        }
        (start.elapsed(), classified)
    };

    let (hoisted, hoisted_count) = timing(&|cell| is_blank(cell, &blank));
    let (as_written, written_count) = timing(&|cell| tested_against_a_fresh_default(cell));
    let (barriered, barriered_count) = timing(&|cell| {
        let fresh = std::hint::black_box(Cell::default());
        cell.c == ' ' && cell.zerowidth().is_none() && Pen::of(cell) == Pen::of(&fresh)
    });
    eprintln!(
        "{} cells x {runs}: hoisted {:?} ({:?}/cell), as written {:?} ({:?}/cell), \
         barriered {:?} ({:?}/cell); classified {hoisted_count}/{written_count}/{barriered_count}",
        cells.len(),
        hoisted,
        hoisted / calls,
        as_written,
        as_written / calls,
        barriered,
        barriered / calls,
    );
}
