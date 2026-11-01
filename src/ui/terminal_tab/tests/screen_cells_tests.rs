use crate::app::App;
use crate::backend::PaneId;
use crate::ui::terminal_tab::render;
use crate::ui::terminal_tab::screen::{ScreenCells, build_screen_lines, screen_cursor_position};
use ratatui::buffer::{Buffer, CellWidth};
use ratatui::layout::{Position, Rect};
use ratatui::widgets::{Paragraph, Widget};
use ratatui::{Terminal, backend::TestBackend};

const AREA: Rect = Rect::new(0, 0, 24, 6);

/// Open one pane and give its emulator a grid of its own, so a test can choose
/// exactly which cells the renderer sees.
fn pane_with_grid(app: &mut App, rows: u16, cols: u16) -> PaneId {
    app.terminal.create_pane_now().unwrap();
    let pane = app.terminal.panes[0].id;
    app.terminal.owns_size = false;
    let emulator = app.terminal.emulators.get_mut(&pane).unwrap();
    emulator.resize(rows, cols);
    pane
}

fn pane_with_output(app: &mut App, rows: u16, cols: u16, output: &[u8]) -> PaneId {
    let pane = pane_with_grid(app, rows, cols);
    app.terminal
        .emulators
        .get_mut(&pane)
        .unwrap()
        .process(output);
    pane
}

fn reference(app: &App, pane: PaneId, area: Rect) -> Buffer {
    let mut buf = Buffer::empty(area);
    let lines = build_screen_lines(app, pane, area.height, area.width);
    Paragraph::new(lines).render(area, &mut buf);
    buf
}

fn widget(app: &App, pane: PaneId, area: Rect) -> Buffer {
    let mut buf = Buffer::empty(area);
    ScreenCells::new(app, pane).render(area, &mut buf);
    buf
}

fn assert_matches_reference(app: &App, pane: PaneId, area: Rect) {
    assert_eq!(
        reference(app, pane, area),
        widget(app, pane, area),
        "widget cells must match the Paragraph reference"
    );
}

fn row_text(buf: &Buffer, y: u16) -> String {
    (buf.area.x..buf.area.right())
        .map(|x| buf[(x, y)].symbol())
        .collect()
}

/// The row with every character that occupies no column removed, so a cell's
/// invisible contents do not mask what the screen shows.
fn visible_text(buf: &Buffer, y: u16) -> String {
    row_text(buf, y)
        .chars()
        .filter(|ch| {
            let mut encoded = [0u8; 4];
            ch.encode_utf8(&mut encoded).cell_width() > 0
        })
        .collect()
}

#[test]
fn plain_ascii_cells_match_the_reference() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(&mut app, 6, 24, b"hello\r\nworld\r\nthird line");

    assert_matches_reference(&app, pane, AREA);
    let buf = widget(&app, pane, AREA);
    assert!(row_text(&buf, 0).starts_with("hello"));
    assert!(row_text(&buf, 1).starts_with("world"));
}

#[test]
fn colors_and_modifiers_match_the_reference() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(
        &mut app,
        6,
        24,
        b"\x1b[31mred\x1b[0m \x1b[44;37mbg\x1b[0m \x1b[1;3;4mattrs\x1b[0m \x1b[2mdim\x1b[0m",
    );

    assert_matches_reference(&app, pane, AREA);
    let buf = widget(&app, pane, AREA);
    assert_eq!(buf[(0, 0)].fg, ratatui::style::Color::Indexed(1));
    assert!(
        buf[(5, 0)].bg == ratatui::style::Color::Indexed(4),
        "the background run keeps its background"
    );
}

#[test]
fn inverse_video_marks_a_selection_like_the_reference() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(&mut app, 6, 24, b"plain \x1b[7mselected\x1b[0m end");

    assert_matches_reference(&app, pane, AREA);
    let selected = widget(&app, pane, AREA);
    assert!(
        selected[(6, 0)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED),
        "reverse video is what marks a vim visual or fzf hit"
    );
    assert!(
        !selected[(0, 0)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );
}

#[test]
fn wide_characters_match_the_reference_and_keep_their_columns() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(
        &mut app,
        6,
        24,
        "日本語 ＡＢＣ 漢字 mixed\r\n二行目".as_bytes(),
    );

    assert_matches_reference(&app, pane, AREA);
    let buf = widget(&app, pane, AREA);
    // The glyph lives on the first cell; the spacer it covers stays blank, so
    // the columns after it keep their own positions.
    assert_eq!(buf[(0, 0)].symbol(), "日");
    assert_eq!(buf[(1, 0)].symbol(), " ");
    assert_eq!(buf[(2, 0)].symbol(), "本");
    assert_eq!(buf[(4, 0)].symbol(), "語");
    assert_eq!(buf[(6, 0)].symbol(), " ");
    assert_eq!(buf[(7, 0)].symbol(), "Ａ");
}

#[test]
fn combining_characters_stay_with_their_base_cell() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(
        &mut app,
        6,
        24,
        "e\u{301}cole a\u{308}\u{301}ok\r\n".as_bytes(),
    );

    assert_matches_reference(&app, pane, AREA);
    let buf = widget(&app, pane, AREA);
    assert_eq!(buf[(0, 0)].symbol(), "e\u{301}");
    assert_eq!(buf[(2, 0)].symbol(), "o");
    assert_eq!(buf[(6, 0)].symbol(), "a\u{308}\u{301}");
    assert_eq!(buf[(7, 0)].symbol(), "o");
    assert_eq!(buf[(8, 0)].symbol(), "k");
}

#[test]
fn a_zero_width_format_character_stays_invisible() {
    let mut app = crate::app::tests::app_with_fake_backend();
    // U+200B forms its own grapheme cluster, so a paragraph drops it while the
    // widget keeps it inside the preceding cell's symbol. Neither draws it, and
    // the columns after it stay put either way.
    let pane = pane_with_output(&mut app, 4, 12, "a\u{200B}b\r\n".as_bytes());

    let reference = reference(&app, pane, AREA);
    let drawn = widget(&app, pane, AREA);
    assert_eq!(visible_text(&reference, 0).trim_end(), "ab");
    assert_eq!(visible_text(&drawn, 0).trim_end(), "ab");
}

#[test]
fn a_row_narrower_than_the_grid_crops_at_its_edge() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(&mut app, 4, 12, b"clipped beyond the pane\r\n");

    let narrow = Rect::new(0, 0, 10, 4);
    assert_matches_reference(&app, pane, narrow);
    let buf = widget(&app, pane, narrow);
    assert_eq!(row_text(&buf, 0), "clipped be");
}

#[test]
fn a_wide_glyph_straddling_the_edge_is_dropped_whole() {
    let mut app = crate::app::tests::app_with_fake_backend();
    // "ab日本" across a five-column area: `本` starts in the last column and
    // cannot fit, so the reference drops it and so must the widget.
    let pane = pane_with_output(&mut app, 4, 12, "ab日本".as_bytes());

    let narrow = Rect::new(0, 0, 5, 4);
    assert_matches_reference(&app, pane, narrow);
    let buf = widget(&app, pane, narrow);
    assert_eq!(row_text(&buf, 0), "ab日  ");
}

#[test]
fn a_grid_smaller_than_the_area_leaves_the_rest_blank() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(&mut app, 2, 5, b"tiny\r\n");

    assert_matches_reference(&app, pane, AREA);
    let buf = widget(&app, pane, AREA);
    assert_eq!(row_text(&buf, 0).trim_end(), "tiny");
    assert!(buf.area.height > 2, "the area is taller than the grid");
}

#[test]
fn a_pane_without_a_screen_draws_the_same_placeholder() {
    let app = crate::app::tests::app_with_fake_backend();
    let missing: PaneId = 9_999;

    assert_matches_reference(&app, missing, AREA);
    // Clipped to a row shorter than the sentence, as the paragraph was.
    let narrow = Rect::new(0, 0, 6, 2);
    assert_matches_reference(&app, missing, narrow);
    let buf = widget(&app, missing, narrow);
    assert_eq!(row_text(&buf, 0), " (no o");
}

#[test]
fn the_cursor_sits_where_the_emulator_says_and_clamps_to_the_area() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let pane = pane_with_output(&mut app, 3, 6, b"ab\r\ncd");

    let screen = app.terminal.screen_for_pane(pane).unwrap();
    assert_eq!(
        screen_cursor_position(&screen, AREA),
        Some(Position::new(2, 1))
    );
    // A grid wider than the drawn area keeps the cursor inside it.
    let narrow = Rect::new(4, 2, 1, 1);
    assert_eq!(
        screen_cursor_position(&screen, narrow),
        Some(Position::new(4, 2))
    );
    assert_eq!(screen_cursor_position(&screen, Rect::new(0, 0, 0, 4)), None);
}

#[test]
fn the_drawn_frame_places_the_host_cursor_and_keeps_styles() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.focus = crate::app::Focus::Terminal;
    let pane = pane_with_output(&mut app, 6, 18, b"\x1b[7mxy\x1b[0mz");
    let area = Rect::new(0, 0, 30, 12);
    let mut terminal = Terminal::new(TestBackend::new(30, 12)).unwrap();

    terminal
        .draw(|frame| render(frame, &app, area, ratatui::style::Color::Yellow))
        .unwrap();

    let (_, content) = crate::ui::terminal_tab::layout::terminal_layout(area).unwrap();
    let cells = crate::ui::terminal_tab::cells::visible_pane_cells(&app, content);
    let pane_area = cells[0].content;
    let screen = app.terminal.screen_for_pane(pane).unwrap();
    assert_eq!(
        terminal.backend().cursor_position(),
        screen_cursor_position(&screen, pane_area).unwrap()
    );
    let buf = terminal.backend().buffer();
    assert_eq!(buf[(pane_area.x, pane_area.y)].symbol(), "x");
}
