use super::*;

#[test]
fn switch_pane_moves_focus_to_terminal() {
    let mut app = app_with_files(vec![]);
    app.terminal.panes = vec![
        PaneInfo {
            id: 1,
            title: "shell 1".into(),
        },
        PaneInfo {
            id: 2,
            title: "shell 2".into(),
        },
    ];
    assert_eq!(app.focus, Focus::FileList);
    app.switch_pane(1);
    assert_eq!(app.focus, Focus::Terminal);
    assert_eq!(app.terminal.active, 1);
}

#[test]
fn open_new_pane_moves_focus_to_new_terminal() {
    let mut app = app_with_fake_backend();
    assert_eq!(app.focus, Focus::FileList);

    app.open_new_pane();

    assert_eq!(app.terminal.panes.len(), 1);
    assert_eq!(app.focus, Focus::Terminal);
    assert_eq!(app.terminal.active, 0);
}

#[test]
fn open_new_pane_exits_competing_fullscreen() {
    let mut app = app_with_fake_backend();
    app.toggle_diff_fullscreen();
    assert!(app.diff.fullscreen);

    app.open_new_pane();

    assert_eq!(app.focus, Focus::Terminal);
    assert!(!app.diff.fullscreen);
    assert!(!app.list_fullscreen);
}

/// Contract for the close/swap availability predicates shared by the key
/// gates (`main::handle_global_action`) and the armed hint row: close
/// needs terminal focus, swap additionally needs a second pane.
#[test]
fn pane_action_predicates_follow_focus_and_pane_count() {
    let mut app = app_with_fake_backend();
    app.terminal.create_pane().unwrap();
    app.terminal.create_pane().unwrap();
    assert!(
        !app.can_close_pane() && !app.can_swap_panes(),
        "neither close nor swap may act without terminal focus"
    );

    app.focus = Focus::Terminal;
    assert!(app.can_close_pane());
    assert!(app.can_swap_panes());

    app.close_active_pane();
    assert!(
        app.can_close_pane(),
        "close still acts on the last remaining pane"
    );
    assert!(
        !app.can_swap_panes(),
        "swap needs a second pane to exchange with"
    );
}

#[test]
fn switch_pane_ignores_out_of_range() {
    let mut app = app_with_files(vec![]);
    app.switch_pane(5);
    assert_eq!(app.terminal.active, 0);
}

#[test]
fn switch_pane_slides_visible_window_to_include_hidden_pane() {
    let mut app = app_with_files(vec![]);
    app.terminal.max_visible_normal = 4;
    app.terminal.panes = (0..7)
        .map(|i| PaneInfo {
            id: i + 1,
            title: format!("shell {}", i + 1),
        })
        .collect();

    // Jumping straight to the last pane (index 6, beyond the default
    // [0,4) window) must slide the window to include it.
    app.switch_pane(6);

    assert_eq!(app.terminal.active, 6);
    assert!(app.terminal.visible_start <= 6 && app.terminal.visible_start + 4 > 6);
}

#[test]
fn closing_pane_reclamps_visible_window() {
    let mut app = app_with_fake_backend();
    app.terminal.max_visible_normal = 4;
    for i in 0..7 {
        app.terminal
            .create_pane_with(None, Some(&format!("P{i}")))
            .unwrap();
    }
    assert_eq!(app.terminal.active, 6);

    // Close panes back down to a single one; the visible window must
    // shrink back to contain only the remaining pane.
    for _ in 0..6 {
        app.close_active_pane();
    }

    assert_eq!(app.terminal.panes.len(), 1);
    assert_eq!(app.terminal.active, 0);
    assert_eq!(app.terminal.visible_start, 0);
}