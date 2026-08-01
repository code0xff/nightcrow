use super::common::*;
use crate::app::App;
use crate::app::tests::app_with_fake_backend;
use crate::app::{Focus, ViewMode};
use crate::ui::hint_bar::{HintClick, hint_click_at};
use crate::ui::status_view::RepoInput;
use ratatui::layout::Rect;

/// `<leader> w` only closes with terminal focus (`handle_global_action`
/// scopes it), so both the armed row and the normal legends must only
/// advertise it there — a hint for a no-op key would lie.
#[test]
fn prefix_hint_advertises_close_only_with_terminal_focus() {
    let mut upper = app_with_fake_backend();
    upper.interaction.prefix_armed = true;
    assert!(
        !hint_text(&upper).contains("w: close"),
        "armed row must not offer close without terminal focus"
    );

    let mut term = app_with_fake_backend();
    term.focus = Focus::Terminal;
    term.interaction.prefix_armed = true;
    assert!(
        hint_text(&term).contains("w: close"),
        "armed row must offer close with terminal focus"
    );
}

/// The armed row's `w: close` must round-trip to a click exactly when it
/// is shown: some column resolves to `Plain('w')` with terminal focus,
/// and no column does without it (the segment isn't rendered, so a click
/// target for it would be a phantom).
#[test]
fn armed_prefix_close_click_target_follows_terminal_focus() {
    let screen = Rect::new(0, 0, 200, 3);
    let clicks = |app: &App| {
        (0..200u16)
            .filter(|&x| {
                hint_click_at(app, plain_chrome(&RepoInput::default()), screen, x, 2)
                    == Some(HintClick::Plain('w'))
            })
            .count()
    };

    let mut term = app_with_fake_backend();
    term.focus = Focus::Terminal;
    term.interaction.prefix_armed = true;
    assert!(
        clicks(&term) > 0,
        "terminal-focused armed row must offer a close click target"
    );

    let mut upper = app_with_fake_backend();
    upper.interaction.prefix_armed = true;
    assert_eq!(
        clicks(&upper),
        0,
        "non-terminal armed row must not resolve any cell to a close click"
    );
}

/// `<leader> s` shares close's scoping (`handle_global_action`): terminal
/// focus plus a second pane to swap with. The armed row must only
/// advertise it then — a hint for a no-op key would lie.
#[test]
fn prefix_hint_advertises_swap_only_when_a_swap_can_act() {
    let mut upper = app_with_fake_backend();
    upper.terminal.create_pane_now().unwrap();
    upper.terminal.create_pane_now().unwrap();
    upper.focus = Focus::FileList;
    upper.interaction.prefix_armed = true;
    assert!(
        !hint_text(&upper).contains("s: swap pane"),
        "armed row must not offer swap without terminal focus"
    );

    let mut single = app_with_fake_backend();
    single.terminal.create_pane_now().unwrap();
    single.focus = Focus::Terminal;
    single.interaction.prefix_armed = true;
    assert!(
        !hint_text(&single).contains("s: swap pane"),
        "armed row must not offer swap with a single pane"
    );

    let mut term = app_with_fake_backend();
    term.terminal.create_pane_now().unwrap();
    term.terminal.create_pane_now().unwrap();
    term.focus = Focus::Terminal;
    term.interaction.prefix_armed = true;
    assert!(
        hint_text(&term).contains("s: swap pane"),
        "armed row must offer swap with terminal focus and two panes"
    );
}

/// The armed row's view toggles name their destination from the current
/// mode, mirroring the normal legends' `l: log view`/`l: status view`
/// wording instead of a generic `log/status` label.
#[test]
fn prefix_hint_names_view_toggle_destinations_by_mode() {
    let mut app = app_with_fake_backend();
    app.interaction.prefix_armed = true;

    let text = hint_text(&app);
    assert!(
        text.contains("l: log view") && text.contains("b: tree view"),
        "status mode armed row must name log/tree destinations, got: {text}"
    );

    app.mode = ViewMode::Log;
    let text = hint_text(&app);
    assert!(
        text.contains("l: status view") && text.contains("b: tree view"),
        "log mode armed row must name status/tree destinations, got: {text}"
    );

    app.mode = ViewMode::Tree;
    let text = hint_text(&app);
    assert!(
        text.contains("l: log view") && text.contains("b: status view"),
        "tree mode armed row must name log/status destinations, got: {text}"
    );
}

/// Every upper legend advertises both view toggles with destination
/// labels — `l` (log ↔ status) and `b` (tree ↔ status) act from any
/// focus, so no mode may hide one or name the view already shown.
#[test]
fn upper_legends_advertise_both_view_toggles() {
    // FileList browsing commits in Log mode.
    let mut app = app_with_fake_backend();
    app.mode = ViewMode::Log;
    let text = hint_text(&app);
    assert!(
        text.contains("l: status view") && text.contains("b: tree view"),
        "log list legend must offer both toggles, got: {text}"
    );

    // DiffViewer in Log mode: `l` names status, not the view shown.
    app.focus = Focus::DiffViewer;
    let text = hint_text(&app);
    assert!(
        text.contains("l: status view") && text.contains("b: tree view"),
        "log diff legend must offer both toggles, got: {text}"
    );

    // Terminal focus in Log mode follows the same destination wording.
    app.focus = Focus::Terminal;
    let text = hint_text(&app);
    assert!(
        text.contains("l: status view"),
        "log terminal legend must name the status destination, got: {text}"
    );

    // Zoomed list rows carry both toggles in every mode.
    let mut zoomed = app_with_fake_backend();
    zoomed.list_fullscreen = true;
    let text = hint_text(&zoomed);
    assert!(
        text.contains("l: log view") && text.contains("b: tree view"),
        "zoomed status list must offer both toggles, got: {text}"
    );
    zoomed.mode = ViewMode::Log;
    let text = hint_text(&zoomed);
    assert!(
        text.contains("l: status view") && text.contains("b: tree view"),
        "zoomed log list must offer both toggles, got: {text}"
    );
    zoomed.mode = ViewMode::Tree;
    let text = hint_text(&zoomed);
    assert!(
        text.contains("b: status view") && text.contains("l: log view"),
        "zoomed tree list must offer both toggles, got: {text}"
    );
}
