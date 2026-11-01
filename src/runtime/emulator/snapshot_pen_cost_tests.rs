//! What the blank-test hoist saves, measured at the predicate that carried it: one
//! comparison per cell of a row's reverse scan, on the screen a snapshot is most
//! expensive for.
//!
//! "As written" is the old source as the compiler saw it, free to lift the default
//! cell out itself; "barriered" pins that construction inside the call, which is what
//! the removal is worth when it cannot be lifted.
//!
//! Measured per cell over 65000 cells and 20 passes:
//!
//! | build | hoisted | as written | barriered |
//! |---|---|---|---|
//! | release | 2 ns | 2 ns | 4 ns |
//! | debug | 59 ns | 94 ns | 88 ns |
//!
//! The optimiser already lifted the constant, so release builds gain nothing and lose
//! nothing; an unoptimised build gains about a third, which is the shape of the work
//! the hoist removes.
//!
//! ```text
//! cargo test --release measure_blank_test -- --ignored --nocapture
//! ```

use super::super::PaneEmulator;
use super::pen_tests::tested_against_a_fresh_default;
use super::{Pen, is_blank};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::Cell;

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
