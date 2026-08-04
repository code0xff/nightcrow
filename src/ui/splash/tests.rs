use super::{draw, draw_idle, scene};
use ratatui::{Terminal, backend::TestBackend, style::Color, text::Line};

fn rows(width: u16, height: u16, draw_into: impl FnOnce(&mut ratatui::Frame)) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(draw_into).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf.cell((x, y)).unwrap().symbol().to_string())
                .collect()
        })
        .collect()
}

fn splash(tick: usize) -> Vec<String> {
    rows(64, 26, |frame| draw(frame, Color::Yellow, tick))
}

fn idle(width: u16, height: u16) -> Vec<String> {
    rows(width, height, |frame| {
        let area = frame.area();
        draw_idle(frame, area, Color::Yellow, Line::from("hint"));
    })
}

/// The bough's row. Its run of solid blocks is longer than any in the bird, so
/// this finds the bough rather than the body.
fn bough_row(rows: &[String]) -> usize {
    rows.iter()
        .position(|row| row.contains("████████████████████████"))
        .expect("the bough is drawn")
}

#[test]
fn the_splash_shows_the_crow_the_moon_and_the_stars() {
    let text = splash(0).join("\n");
    assert!(text.contains('●'), "missing the crow's eye:\n{text}");
    assert!(text.contains('▟'), "missing the crescent moon:\n{text}");
    assert!(
        text.contains('*') || text.contains('·'),
        "missing stars:\n{text}"
    );
}

#[test]
fn the_splash_names_the_version_and_the_way_out() {
    let text = splash(0).join("\n");
    assert!(text.contains("nightcrow"), "missing product name:\n{text}");
    assert!(
        text.contains(&format!("v{}", env!("CARGO_PKG_VERSION"))),
        "missing version:\n{text}"
    );
    assert!(
        text.contains("Press any key to continue"),
        "missing dismissal prompt:\n{text}"
    );
}

#[test]
fn only_the_sky_moves_while_the_splash_waits() {
    let first = splash(0);
    let later = splash(3);
    assert_ne!(first, later, "the sky never changed");

    let bough = bough_row(&first);
    assert_eq!(bough_row(&later), bough, "the bough moved");
    assert_eq!(
        first[bough..bough + 2],
        later[bough..bough + 2],
        "the bough changed between frames"
    );

    let eye = |rows: &[String]| {
        rows.iter()
            .enumerate()
            .find_map(|(y, row)| row.find('●').map(|x| (y, x)))
    };
    assert_eq!(eye(&first), eye(&later), "the crow moved");
}

#[test]
fn a_terminal_too_small_for_the_scene_still_draws() {
    let text = rows(scene::WIDTH as u16 / 2, 8, |frame| {
        draw(frame, Color::Yellow, 3)
    })
    .join("\n");
    assert!(text.contains("nightcrow"), "the text must survive:\n{text}");
}

#[test]
fn an_empty_pane_puts_the_hint_under_the_scene() {
    let rows = idle(72, 22);
    let text = rows.join("\n");
    assert!(text.contains('●'), "the crow is missing:\n{text}");

    let bough = bough_row(&rows);
    let hint = rows.iter().position(|row| row.contains("hint")).unwrap();
    assert!(bough < hint, "expected the scene above the hint");
}

#[test]
fn a_short_empty_pane_keeps_the_hint_and_drops_the_scene() {
    let text = idle(72, 6).join("\n");
    assert!(text.contains("hint"), "the hint must survive:\n{text}");
    assert!(!text.contains('●'), "no room for the crow here:\n{text}");
}

#[test]
fn an_empty_pane_narrower_than_the_scene_still_shows_the_hint() {
    let text = idle(scene::WIDTH as u16 - 4, 22).join("\n");
    assert!(text.contains("hint"), "the hint must survive:\n{text}");
    assert!(!text.contains('●'), "no room for the crow here:\n{text}");
}

#[test]
fn a_one_row_pane_shows_the_hint_alone() {
    let rows = idle(72, 1);
    assert!(rows[0].contains("hint"), "{rows:?}");
}
