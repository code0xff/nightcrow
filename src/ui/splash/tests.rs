use super::{crow, draw};
use ratatui::{Terminal, backend::TestBackend, style::Color};

const W: u16 = 60;
const H: u16 = 30;

fn rows_at(tick: usize) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(W, H)).unwrap();
    terminal
        .draw(|frame| draw(frame, Color::Yellow, tick))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..H)
        .map(|y| {
            (0..W)
                .map(|x| buf.cell((x, y)).unwrap().symbol().to_string())
                .collect()
        })
        .collect()
}

/// Columns of the branch and of the tail tip just above it — parts of the logo
/// no wing touches, so they pin down where the whole block was drawn.
fn anchor_columns(rows: &[String]) -> (usize, usize) {
    let branch = rows
        .iter()
        .position(|row| row.contains('─'))
        .expect("the branch row is drawn");
    let column = |row: &String| row.find(|ch: char| ch != ' ').expect("row is not blank");
    (column(&rows[branch]), column(&rows[branch - 1]))
}

#[test]
fn the_logo_keeps_its_column_while_the_wing_sweeps() {
    let first = anchor_columns(&rows_at(0));
    for tick in 1..10 {
        assert_eq!(
            anchor_columns(&rows_at(tick)),
            first,
            "frame {tick} shifted the splash horizontally"
        );
    }
}

#[test]
fn the_wing_moves_between_frames() {
    let raised = rows_at(4);
    let lowered = rows_at(0);
    assert_ne!(raised, lowered, "the flap did not change the drawn crow");
}

#[test]
fn the_splash_names_the_crow_and_the_way_out() {
    let text = rows_at(0).join("\n");
    assert!(text.contains("nightcrow"), "missing product name:\n{text}");
    assert!(
        text.contains("Press any key to continue"),
        "missing dismissal prompt:\n{text}"
    );
    assert!(text.contains('●'), "missing crow eye:\n{text}");
}

#[test]
fn a_terminal_narrower_than_the_logo_still_draws() {
    let mut terminal = Terminal::new(TestBackend::new(crow::WIDTH as u16 / 2, 8)).unwrap();
    terminal
        .draw(|frame| draw(frame, Color::Yellow, 3))
        .unwrap();
}
