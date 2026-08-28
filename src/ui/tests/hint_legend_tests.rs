use super::common::*;
use crate::app::Focus;
use crate::app::tests::app_with_fake_backend;
use crate::runtime::terminal::TerminalFullscreen;
use crate::ui::hint_bar::{HintClick, hint_click_at, render_hint_bar, segment_click};
use crate::ui::status_view::RepoInput;
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
};

#[test]
fn hint_bar_inverts_only_clickable_key_labels() {
    let app = app_with_fake_backend();
    assert_inverted_cells_are_clickable(&app);
}
#[test]
fn armed_prefix_hint_advertises_the_project_keys() {
    let mut app = app_with_fake_backend();
    app.interaction.prefix_armed = true;

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

/// Every command hint dispatches, per `docs/keybindings.md` — these five were
/// commands the list left out, so a click on them did nothing while their
/// neighbours on the same row worked.
#[test]
fn every_command_keyspec_resolves_to_a_click() {
    for spec in ["u", "z", "c", "n"] {
        let c = spec.chars().next().unwrap();
        assert_eq!(
            segment_click(spec),
            Some(HintClick::Plain(c)),
            "`{spec}` names a command, so it must be clickable"
        );
    }
    assert_eq!(
        segment_click("shift+n"),
        Some(HintClick::Plain('N')),
        "the chord resolves to the character its handler matches on"
    );
}

/// The counterpart: navigation keyspecs and `detach` stay unclickable. Named
/// here so widening the key list cannot quietly swallow one of them.
#[test]
fn navigation_and_detach_keyspecs_resolve_to_nothing() {
    for spec in [
        "j/k",
        "pgup/pgdn",
        "shift+up/dn",
        "shift+pgup/dn",
        "shift+left/right",
        "enter",
        "esc",
        "tab",
        "left",
        "right",
        "1-8",
        "1-9",
        "q",
        "<prefix> q",
    ] {
        assert_eq!(segment_click(spec), None, "`{spec}` must not be clickable");
    }
}

#[test]
fn armed_prefix_hint_bar_inverts_only_clickable_key_labels() {
    let mut app = app_with_fake_backend();
    app.interaction.prefix_armed = true;
    assert_inverted_cells_are_clickable(&app);
}
/// The two rows carrying the newly clickable labels, held to the same
/// invariant as the rest: a label renders inverted exactly when it is
/// clickable, so widening the key list cannot promise a click the hit test
/// will not answer.
#[test]
fn rows_carrying_the_added_commands_invert_only_clickable_labels() {
    let mut armed = app_with_fake_backend();
    armed.interaction.prefix_armed = true;
    armed.terminal.owns_size = false;
    armed.terminal.recovery.insert(
        0,
        crate::runtime::terminal::PaneRecovery {
            state: "waiting_for_reset".to_string(),
            detail: None,
            deadline_epoch: None,
            attempt: 1,
        },
    );
    assert_inverted_cells_are_clickable(&armed);

    let mut searched = app_with_fake_backend();
    searched.focus = Focus::DiffViewer;
    searched.git.view.diff.search.query.set("foo");
    assert_inverted_cells_are_clickable(&searched);
}

#[test]
fn hint_bar_inverts_nothing_when_mouse_capture_is_disabled() {
    let mut app = app_with_fake_backend();
    app.interaction.mouse_enabled = false;
    let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render_hint_bar(
                    &app,
                    plain_chrome(&RepoInput::default()),
                    Color::Yellow,
                    frame.area().width,
                ),
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
    app.interaction.mouse_enabled = false;
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
    app.interaction.begin_swap_target();

    assert!(
        hint_text(&app).contains("3-9,0: swap active pane"),
        "split view swap prompt must advertise the 3-9,0 mapping"
    );
}

#[test]
fn swap_hint_advertises_fullscreen_digits_when_terminal_fills_body() {
    let mut app = app_with_fake_backend();
    app.terminal.fullscreen = TerminalFullscreen::Grid;
    app.interaction.begin_swap_target();

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
    split.interaction.prefix_armed = true;
    assert!(
        hint_text(&split).contains("1-9: focus/pane"),
        "split view prefix hint must advertise focus/pane digits"
    );

    let mut full = app_with_fake_backend();
    full.terminal.fullscreen = TerminalFullscreen::Grid;
    full.interaction.prefix_armed = true;
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
