use super::common::*;
use crate::app::tests::app_with_fake_backend;
use crate::app::Focus;
use crate::runtime::terminal::TerminalFullscreen;
use crate::ui::hint_bar::{HintClick, hint_click_at, render_hint_bar, segment_click};
use crate::ui::status_view::RepoInput;
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::{Color, Modifier}};

#[test]
fn hint_bar_inverts_only_clickable_key_labels() {
    let app = app_with_fake_backend();
    assert_inverted_cells_are_clickable(&app);
}
#[test]
fn armed_prefix_hint_advertises_the_project_keys() {
    let mut app = app_with_fake_backend();
    app.arm_prefix();

    let text = hint_text(&app);

    assert!(text.contains("o: open project"), "got: {text}");
    assert!(text.contains("x: close project"), "got: {text}");
}

/// The terminal-focus legend is the one carrying the bare
/// `<prefix>: leader` segment — its inversion must round-trip to a
/// click like every other clickable label.
#[test]
fn terminal_focus_hint_bar_inverts_only_clickable_key_labels() {
    let mut app = app_with_fake_backend();
    app.focus = Focus::Terminal;
    assert_inverted_cells_are_clickable(&app);
}

#[test]
fn bare_prefix_segment_resolves_to_an_arm_click() {
    assert_eq!(segment_click("<prefix>"), Some(HintClick::Arm));
    assert_eq!(segment_click(" <prefix>"), Some(HintClick::Arm));
}

#[test]
fn armed_prefix_hint_bar_inverts_only_clickable_key_labels() {
    let mut app = app_with_fake_backend();
    app.arm_prefix();
    assert_inverted_cells_are_clickable(&app);
}
#[test]
fn hint_bar_inverts_nothing_when_mouse_capture_is_disabled() {
    let mut app = app_with_fake_backend();
    app.mouse_enabled = false;
    let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render_hint_bar(&app, plain_chrome(&RepoInput::default()), Color::Yellow),
                frame.area(),
            )
        })
        .unwrap();
    let buf = terminal.backend().buffer();

    let inverted = (0..200u16).any(|x| buf[(x, 0)].modifier.contains(Modifier::REVERSED));

    assert!(
        !inverted,
        "with the mouse handed back to the terminal, no hint may \
         advertise a click that cannot arrive"
    );
}

/// The affordance/hit-test agreement holds with the mouse disabled too:
/// nothing renders inverted, so nothing may resolve to a click.
#[test]
fn hint_click_resolves_nothing_when_mouse_capture_is_disabled() {
    let mut app = app_with_fake_backend();
    app.focus = Focus::Terminal;
    app.mouse_enabled = false;
    let screen = Rect::new(0, 0, 200, 3);
    for x in 0..200u16 {
        assert_eq!(
            hint_click_at(&app, plain_chrome(&RepoInput::default()), screen, x, 2),
            None,
            "x={x} resolves to a click the disabled mouse can never send"
        );
    }
}
#[test]
fn swap_hint_advertises_split_view_digits_by_default() {
    let mut app = app_with_fake_backend();
    app.begin_swap_target();

    assert!(
        hint_text(&app).contains("3-9,0: swap active pane"),
        "split view swap prompt must advertise the 3-9,0 mapping"
    );
}

#[test]
fn swap_hint_advertises_fullscreen_digits_when_terminal_fills_body() {
    let mut app = app_with_fake_backend();
    app.terminal.fullscreen = TerminalFullscreen::Grid;
    app.begin_swap_target();

    let text = hint_text(&app);
    assert!(
        text.contains("1-8: swap active pane"),
        "fullscreen swap prompt must advertise the 1-8 mapping, got: {text}"
    );
    assert!(
        !text.contains("3-9,0"),
        "fullscreen swap prompt must not show the split-view digits, got: {text}"
    );
}
#[test]
fn prefix_hint_switches_pane_digit_legend_by_layout() {
    let mut split = app_with_fake_backend();
    split.arm_prefix();
    assert!(
        hint_text(&split).contains("1-9: focus/pane"),
        "split view prefix hint must advertise focus/pane digits"
    );

    let mut full = app_with_fake_backend();
    full.terminal.fullscreen = TerminalFullscreen::Grid;
    full.arm_prefix();
    let text = hint_text(&full);
    assert!(
        text.contains("1-8: pane"),
        "fullscreen prefix hint must advertise the 1-8 pane digits, got: {text}"
    );
    assert!(
        !text.contains("1-9: focus/pane"),
        "fullscreen prefix hint must not show the split-view legend, got: {text}"
    );
}
