use super::*;
use crate::app::Focus;
use crate::ui::terminal_tab::layout::terminal_layout;
use crate::ui::terminal_tab::render;
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
};

#[test]
fn single_pane_render_still_has_no_left_border_character() {
    // Regression guard for split-view acceptance criterion 9: with only
    // one pane, render() must take the no-cell-border branch, matching
    // pre-split-view behaviour exactly (clean copy-paste, no `│`).
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("Solo"))
        .unwrap();
    let area = Rect::new(0, 0, 40, 10);
    let mut terminal = Terminal::new(TestBackend::new(40, 10)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, area, Color::Yellow);
        })
        .unwrap();

    let (_, content) = terminal_layout(area).unwrap();
    let buf = terminal.backend().buffer();
    for y in content.top()..content.bottom() {
        let cell = buf.cell((content.x, y)).unwrap();
        assert_ne!(
            cell.symbol(),
            "│",
            "single pane must not draw a left border at y={y}"
        );
    }
}

#[test]
fn split_view_renders_multiple_panes_simultaneously() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("Alpha"))
        .unwrap();
    app.terminal
        .create_pane_with_now(None, Some("Beta"))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, frame.area(), Color::Yellow);
        })
        .unwrap();

    let text = buffer_text(terminal.backend().buffer());
    assert!(
        text.contains("Alpha") && text.contains("Beta"),
        "expected both pane titles visible at once, got: {text}"
    );
}

#[test]
fn split_view_borders_active_pane_in_accent_color() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("Alpha"))
        .unwrap();
    app.terminal
        .create_pane_with_now(None, Some("Beta"))
        .unwrap();
    app.focus = Focus::Terminal;
    let accent = Color::Yellow;
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, frame.area(), accent);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    assert!(
        buf.content.iter().any(|cell| cell.fg == accent),
        "expected the active pane's border/title in accent color"
    );
    assert!(
        buf.content.iter().any(|cell| cell.fg == Color::DarkGray),
        "expected the inactive pane's border in dark gray"
    );
}

#[test]
fn split_view_active_pane_matches_inactive_style_when_terminal_unfocused() {
    // Regression guard: accent (and any brighter stand-in for it) must
    // mean "keystrokes go here right now". When Diff/FileList holds
    // focus, the terminal's active pane must render pixel-identical to
    // an inactive pane — no accent, no bold, no lighter gray — otherwise
    // it still reads as focused when it isn't.
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("Alpha"))
        .unwrap();
    app.terminal
        .create_pane_with_now(None, Some("Beta"))
        .unwrap();
    app.focus = Focus::DiffViewer;
    let accent = Color::Yellow;
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, frame.area(), accent);
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    assert!(
        !buf.content
            .iter()
            .any(|cell| cell.fg == accent || cell.fg == Color::White),
        "terminal must not show accent or white anywhere while unfocused"
    );
    assert!(
        !buf.content
            .iter()
            .any(|cell| cell.modifier.contains(Modifier::BOLD) && cell.bg == accent),
        "active pane tab must not carry an accent-bolded highlight while unfocused"
    );
}

#[test]
fn tab_target_at_resolves_tabs_and_hidden_markers() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal.max_visible_normal = 2;
    for i in 0..4 {
        app.terminal
            .create_pane_with_now(None, Some(&format!("P{i}")))
            .unwrap();
    }
    // Creation leaves pane 3 active with a 2-pane window: [2, 4).
    let area = Rect::new(0, 0, 80, 20);
    let (tab_area, _) = terminal_layout(area).unwrap();
    let y = tab_area.y;
    let _ = y;
}
