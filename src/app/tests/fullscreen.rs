use super::*;

#[test]
fn focus_list_jumps_and_exits_competing_fullscreens() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.focus = Focus::Terminal;
    app.toggle_terminal_fullscreen();
    assert!(app.terminal.fullscreen.fills_body());

    app.focus_list();

    assert_eq!(app.focus, Focus::FileList);
    assert!(!app.terminal.fullscreen.fills_body());
    assert!(!app.diff.fullscreen);
}

#[test]
fn focus_diff_jumps_and_exits_competing_fullscreens() {
    let mut app = app_with_files(vec![]);
    app.toggle_list_fullscreen();
    assert!(app.list_fullscreen);

    app.focus_diff();

    assert_eq!(app.focus, Focus::DiffViewer);
    assert!(!app.list_fullscreen);
    assert!(!app.terminal.fullscreen.fills_body());
}

#[test]
fn switch_pane_exits_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_diff_fullscreen();
    assert!(app.diff.fullscreen);

    app.switch_pane(0);

    assert!(!app.diff.fullscreen);
    assert_eq!(app.focus, Focus::Terminal);
    assert_eq!(app.terminal.active, 0);
}

#[test]
fn toggle_fullscreen_switches_focus_to_terminal() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    assert_eq!(app.focus, Focus::FileList);

    app.toggle_terminal_fullscreen();

    assert!(app.terminal.fullscreen.fills_body());
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn toggle_fullscreen_noop_with_no_panes() {
    let mut app = app_with_files(vec![]);
    assert!(app.terminal.panes.is_empty());

    app.toggle_terminal_fullscreen();

    assert!(!app.terminal.fullscreen.fills_body());
}

#[test]
fn toggle_terminal_fullscreen_cycles_off_grid_zoom_off_with_multiple_panes() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![
        PaneInfo {
            id: 1,
            title: "a".into(),
        },
        PaneInfo {
            id: 2,
            title: "b".into(),
        },
    ];
    app.focus = Focus::Terminal;
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Off);

    app.toggle_terminal_fullscreen();
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Grid);

    app.toggle_terminal_fullscreen();
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Zoom);
    // Zoom pins the visible window to exactly the active pane.
    assert_eq!(app.terminal.max_visible(), 1);

    app.toggle_terminal_fullscreen();
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Off);
}

#[test]
fn closing_pane_normalizes_zoom_to_grid_when_one_pane_remains() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![
        PaneInfo {
            id: 1,
            title: "a".into(),
        },
        PaneInfo {
            id: 2,
            title: "b".into(),
        },
    ];
    app.focus = Focus::Terminal;
    app.toggle_terminal_fullscreen(); // Grid
    app.toggle_terminal_fullscreen(); // Zoom
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Zoom);

    // One pane left: Zoom is indistinguishable from Grid, so it normalizes.
    app.terminal.panes.pop();
    app.clamp_active_pane_after_removal();

    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Grid);
}

#[test]
fn toggle_terminal_fullscreen_skips_zoom_with_single_pane() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.focus = Focus::Terminal;

    app.toggle_terminal_fullscreen();
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Grid);

    // With a lone pane Grid and Zoom look identical, so the cycle collapses
    // straight back to Off rather than stopping at an indistinguishable Zoom.
    app.toggle_terminal_fullscreen();
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Off);
}

#[test]
fn toggle_terminal_fullscreen_skips_zoom_when_grid_cap_is_one() {
    // Even with multiple panes, a fullscreen grid capped at 1 shows a
    // single pane, so Grid and Zoom are indistinguishable and Zoom is
    // skipped — the skip must not assume `max_visible_fullscreen >= 2`.
    let mut app = app_with_files(vec![]);
    app.terminal.max_visible_fullscreen = 1;
    app.terminal.panes = vec![
        PaneInfo {
            id: 1,
            title: "a".into(),
        },
        PaneInfo {
            id: 2,
            title: "b".into(),
        },
    ];
    app.focus = Focus::Terminal;

    app.toggle_terminal_fullscreen();
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Grid);

    app.toggle_terminal_fullscreen();
    assert_eq!(app.terminal.fullscreen, TerminalFullscreen::Off);
}

#[test]
fn toggle_diff_fullscreen_sets_flag_and_focuses_diff_viewer() {
    let mut app = app_with_files(vec![]);
    assert_eq!(app.focus, Focus::FileList);

    app.toggle_diff_fullscreen();

    assert!(app.diff.fullscreen);
    assert_eq!(app.focus, Focus::DiffViewer);

    app.toggle_diff_fullscreen();

    assert!(!app.diff.fullscreen);
    // Exiting zoom leaves focus on DiffViewer (no reason to bounce back).
    assert_eq!(app.focus, Focus::DiffViewer);
}

#[test]
fn toggle_diff_fullscreen_exits_terminal_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_terminal_fullscreen();
    assert!(app.terminal.fullscreen.fills_body());

    app.toggle_diff_fullscreen();

    assert!(app.diff.fullscreen);
    assert!(!app.terminal.fullscreen.fills_body());
    assert_eq!(app.focus, Focus::DiffViewer);
}

#[test]
fn toggle_terminal_fullscreen_exits_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_diff_fullscreen();
    assert!(app.diff.fullscreen);

    app.toggle_terminal_fullscreen();

    assert!(app.terminal.fullscreen.fills_body());
    assert!(!app.diff.fullscreen);
    assert_eq!(app.focus, Focus::Terminal);
}

#[test]
fn cycle_focus_is_noop_in_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![PaneInfo {
        id: 1,
        title: "shell".into(),
    }];
    app.toggle_diff_fullscreen();
    assert_eq!(app.focus, Focus::DiffViewer);

    app.cycle_focus_forward();
    assert_eq!(app.focus, Focus::DiffViewer);

    app.cycle_focus_backward();
    assert_eq!(app.focus, Focus::DiffViewer);
}

#[test]
fn cycle_focus_forward_through_terminal_panes_slides_visible_window() {
    let mut app = app_with_files(vec![]);
    app.terminal.max_visible_normal = 4;
    app.terminal.panes = (0..7)
        .map(|i| PaneInfo {
            id: i + 1,
            title: format!("shell {}", i + 1),
        })
        .collect();
    app.focus = Focus::DiffViewer;

    // DiffViewer -> Terminal(0), then step forward through every pane.
    for expected_active in 0..7 {
        app.cycle_focus_forward();
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.terminal.active, expected_active);
        assert!(
            app.terminal.visible_start <= expected_active
                && app.terminal.visible_start + 4 > expected_active,
            "active {expected_active} not inside visible window starting at {}",
            app.terminal.visible_start
        );
    }
}

#[test]
fn toggle_list_fullscreen_sets_flag_and_focuses_file_list() {
    let mut app = app_with_files(vec![]);
    app.focus = Focus::DiffViewer;
    assert!(!app.list_fullscreen);

    app.toggle_list_fullscreen();

    assert!(app.list_fullscreen);
    assert_eq!(app.focus, Focus::FileList);

    app.toggle_list_fullscreen();

    assert!(!app.list_fullscreen);
    // Exiting list zoom leaves focus on FileList (matches diff zoom semantics).
    assert_eq!(app.focus, Focus::FileList);
}

#[test]
fn toggle_list_fullscreen_exits_diff_fullscreen() {
    let mut app = app_with_files(vec![]);
    app.toggle_diff_fullscreen();
    assert!(app.diff.fullscreen);

    app.toggle_list_fullscreen();

    assert!(app.list_fullscreen);
    assert!(!app.diff.fullscreen);
    assert_eq!(app.focus, Focus::FileList);
}
