use super::common::*;
use super::*;

#[test]
fn max_visible_switches_with_fullscreen() {
    let mut state = state_with_fake();
    state.max_visible_normal = 4;
    state.max_visible_fullscreen = 7;
    assert_eq!(state.max_visible(), 4);
    state.fullscreen = TerminalFullscreen::Grid;
    assert_eq!(state.max_visible(), 7);
    state.fullscreen = TerminalFullscreen::Zoom;
    assert_eq!(state.max_visible(), 1);
}

#[test]
fn visible_range_shows_everything_under_the_cap() {
    assert_eq!(visible_range(0, 0, 3, 4), 0..3);
}

#[test]
fn visible_range_keeps_active_inside_a_capped_window() {
    // 7 panes, window of 4, active is the last pane: window must end at 7.
    assert_eq!(visible_range(0, 6, 7, 4), 3..7);
}

#[test]
fn visible_range_moves_start_forward_only_as_far_as_needed() {
    // Previously showing [2,6). Active moves to 6 (just past the window):
    // start should shift by exactly 1, not jump to re-center.
    assert_eq!(visible_range(2, 6, 7, 4), 3..7);
}

#[test]
fn visible_range_moves_start_backward_when_active_precedes_window() {
    // Previously showing [3,7). Active jumps back to 0.
    assert_eq!(visible_range(3, 0, 7, 4), 0..4);
}

#[test]
fn visible_range_empty_when_no_panes() {
    assert_eq!(visible_range(0, 0, 0, 4), 0..0);
}

#[test]
fn sync_visible_window_follows_active_when_panes_exceed_max_visible() {
    let mut state = state_with_fake();
    state.max_visible_normal = 4;
    for i in 0..7 {
        state
            .create_pane_with(None, Some(&format!("P{i}")))
            .unwrap();
    }
    // Each create_pane_with call makes the new pane active and syncs the
    // window, so after 7 panes the last one (index 6) must be visible.
    assert_eq!(state.active, 6);
    assert!(state.visible_start <= 6 && state.visible_start + 4 > 6);
}

#[test]
fn sync_visible_window_clamps_after_pane_count_shrinks() {
    let mut state = state_with_fake();
    state.max_visible_normal = 4;
    for i in 0..7 {
        state
            .create_pane_with(None, Some(&format!("P{i}")))
            .unwrap();
    }
    // Window is currently sliding near the end; drop back to a single
    // pane and re-sync — start must fall back inside [0, 0].
    state.panes.truncate(1);
    state.active = 0;
    state.sync_visible_window();
    assert_eq!(state.visible_start, 0);
}

#[test]
fn active_pane_rows_uses_pane_specific_size() {
    let mut state = state_with_fake();
    state.create_pane().unwrap();
    let id = state.panes[0].id;
    state.resize_visible_panes(&[(id, 33, 90)]);
    assert_eq!(state.active_pane_rows(), 33);
}

#[test]
fn resize_visible_panes_keeps_default_size_in_sync_with_active_pane() {
    let mut state = state_with_fake();
    state.create_pane().unwrap();
    let first_id = state.panes[0].id;
    state.create_pane().unwrap();
    let second_id = state.panes[1].id;
    state.active = 1;

    state.resize_visible_panes(&[(first_id, 10, 40), (second_id, 12, 50)]);

    assert_eq!(state.size, (12, 50));
}

#[test]
fn active_pane_rows_falls_back_to_default_with_no_panes() {
    let state = state_with_fake();
    assert_eq!(state.active_pane_rows(), state.size.0 as usize);
}