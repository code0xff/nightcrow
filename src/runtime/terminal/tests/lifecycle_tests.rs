use super::common::*;

#[test]
fn create_pane_defaults_to_shell_label_and_no_command() {
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    assert_eq!(state.panes.len(), 1);
    assert_eq!(state.panes[0].title, "shell 1");
}

#[test]
fn create_pane_with_label_sets_title() {
    let mut state = state_with_fake();
    state
        .create_pane_with_now(Some("claude --foo"), Some("Claude"))
        .unwrap();
    assert_eq!(state.panes[0].title, "Claude");
}

#[test]
fn create_pane_with_falls_back_to_command_text() {
    let mut state = state_with_fake();
    state
        .create_pane_with_now(Some("cargo test"), None)
        .unwrap();
    assert_eq!(state.panes[0].title, "cargo test");
}

#[test]
fn create_pane_with_appends_and_focuses_new_pane() {
    let mut state = state_with_fake();
    state
        .create_pane_with_now(Some("echo hi"), Some("E"))
        .unwrap();
    state.create_pane_now().unwrap();
    assert_eq!(state.panes.len(), 2);
    assert_eq!(state.panes[1].title, "shell 2");
    assert_eq!(state.active, 1);
}

#[test]
fn pane_size_falls_back_to_default_before_any_resize() {
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;
    assert_eq!(state.pane_size(id), state.size);
}

#[test]
fn resize_visible_panes_updates_parser_and_last_content_size() {
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    state.resize_visible_panes(&[(id, 12, 60)]);

    assert_eq!(state.screen_for_pane(id).unwrap().size(), (12, 60));
    assert_eq!(state.last_content_size.get(&id), Some(&(12, 60)));
}

#[test]
fn resize_visible_panes_clamps_zero_to_minimum_grid() {
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;

    state.resize_visible_panes(&[(id, 0, 0)]);

    // The recorded size must match the emulator's minimum grid (1x2),
    // not a raw 1x1 clamp — PTY, emulator, and bookkeeping stay in sync.
    assert_eq!(state.last_content_size.get(&id), Some(&(1, 2)));
    assert_eq!(state.screen_for_pane(id).unwrap().size(), (1, 2));
}

#[test]
fn resize_visible_panes_ignores_panes_not_listed() {
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    let hidden_id = state.panes[0].id;
    let hidden_size_at_creation = state.pane_size(hidden_id);
    state.create_pane_now().unwrap();
    let visible_id = state.panes[1].id;

    state.resize_visible_panes(&[(visible_id, 15, 70)]);

    // The hidden pane keeps whatever size it had before this call — it
    // wasn't in the `layouts` list, so `resize_visible_panes` must not
    // touch it.
    assert_eq!(
        state.last_content_size.get(&hidden_id),
        Some(&hidden_size_at_creation)
    );
    assert_eq!(state.last_content_size.get(&visible_id), Some(&(15, 70)));
}

#[test]
fn new_pane_seeds_size_from_active_pane_last_content_size() {
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    let first_id = state.panes[0].id;
    state.resize_visible_panes(&[(first_id, 18, 65)]);

    state.create_pane_now().unwrap();
    let second_id = state.panes[1].id;

    assert_eq!(state.screen_for_pane(second_id).unwrap().size(), (18, 65));
}

#[test]
fn screen_for_pane_none_for_unknown_id() {
    let state = state_with_fake();
    assert!(state.screen_for_pane(999).is_none());
}

#[test]
fn closing_pane_drops_its_last_content_size() {
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    let id = state.panes[0].id;
    state.resize_visible_panes(&[(id, 10, 40)]);

    state.close_active_now();

    assert!(!state.last_content_size.contains_key(&id));
}
