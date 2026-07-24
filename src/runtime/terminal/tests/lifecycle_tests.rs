use super::common::*;
use super::*;

#[test]
fn create_pane_defaults_to_shell_label_and_no_command() {
    let mut state = state_with_fake();
    state.create_pane().unwrap();
    assert_eq!(state.panes.len(), 1);
    assert_eq!(state.panes[0].title, "shell 1");
}

#[test]
fn create_pane_with_label_sets_title() {
    let mut state = state_with_fake();
    state
        .create_pane_with(Some("claude --foo"), Some("Claude"))
        .unwrap();
    assert_eq!(state.panes[0].title, "Claude");
}

#[test]
fn create_pane_with_falls_back_to_command_text() {
    let mut state = state_with_fake();
    state.create_pane_with(Some("cargo test"), None).unwrap();
    assert_eq!(state.panes[0].title, "cargo test");
}

#[test]
fn create_pane_with_appends_and_focuses_new_pane() {
    let mut state = state_with_fake();
    state.create_pane_with(Some("echo hi"), Some("E")).unwrap();
    state.create_pane().unwrap();
    assert_eq!(state.panes.len(), 2);
    assert_eq!(state.panes[1].title, "shell 2");
    assert_eq!(state.active, 1);
}

#[test]
fn swap_active_with_exchanges_panes_and_follows_focus() {
    let mut state = state_with_fake();
    state.create_pane_with(None, Some("A")).unwrap();
    state.create_pane_with(None, Some("B")).unwrap();
    state.create_pane_with(None, Some("C")).unwrap();
    state.active = 0; // focus pane "A"
    let a_id = state.panes[0].id;
    let c_id = state.panes[2].id;

    assert!(state.swap_active_with(2));

    // "A" and "C" exchanged slots; focus followed "A" to slot 2.
    assert_eq!(state.panes[0].id, c_id);
    assert_eq!(state.panes[2].id, a_id);
    assert_eq!(state.panes[0].title, "C");
    assert_eq!(state.panes[2].title, "A");
    assert_eq!(state.active, 2);
}

#[test]
fn swap_active_with_out_of_range_is_noop() {
    let mut state = state_with_fake();
    state.create_pane_with(None, Some("A")).unwrap();
    state.create_pane_with(None, Some("B")).unwrap();
    state.active = 0;

    assert!(!state.swap_active_with(5));
    assert_eq!(state.active, 0);
    assert_eq!(state.panes[0].title, "A");
    assert_eq!(state.panes[1].title, "B");
}

#[test]
fn swap_active_with_self_is_noop() {
    let mut state = state_with_fake();
    state.create_pane_with(None, Some("A")).unwrap();
    state.create_pane_with(None, Some("B")).unwrap();
    state.active = 1;

    assert!(!state.swap_active_with(1));
    assert_eq!(state.active, 1);
    assert_eq!(state.panes[1].title, "B");
}

#[test]
fn swap_active_with_preserves_per_pane_state() {
    let mut state = state_with_fake();
    state.create_pane_with(None, Some("A")).unwrap();
    state.create_pane_with(None, Some("B")).unwrap();
    state.active = 0;
    let a_id = state.panes[0].id;
    // Seed scroll/size state keyed by the moving pane's id.
    state.scroll.insert(a_id, 7);
    state.last_content_size.insert(a_id, (10, 40));

    assert!(state.swap_active_with(1));

    // Per-pane state is id-keyed, so it survives the reorder unchanged.
    assert_eq!(state.scroll.get(&a_id), Some(&7));
    assert_eq!(state.last_content_size.get(&a_id), Some(&(10, 40)));
    assert_eq!(state.panes[1].id, a_id);
}

#[test]
fn pane_size_falls_back_to_default_before_any_resize() {
    let mut state = state_with_fake();
    state.create_pane().unwrap();
    let id = state.panes[0].id;
    assert_eq!(state.pane_size(id), state.size);
}

#[test]
fn resize_visible_panes_updates_parser_and_last_content_size() {
    let mut state = state_with_fake();
    state.create_pane().unwrap();
    let id = state.panes[0].id;

    state.resize_visible_panes(&[(id, 12, 60)]);

    assert_eq!(state.screen_for_pane(id).unwrap().size(), (12, 60));
    assert_eq!(state.last_content_size.get(&id), Some(&(12, 60)));
}

#[test]
fn resize_visible_panes_clamps_zero_to_minimum_grid() {
    let mut state = state_with_fake();
    state.create_pane().unwrap();
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
    state.create_pane().unwrap();
    let hidden_id = state.panes[0].id;
    let hidden_size_at_creation = state.pane_size(hidden_id);
    state.create_pane().unwrap();
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
    state.create_pane().unwrap();
    let first_id = state.panes[0].id;
    state.resize_visible_panes(&[(first_id, 18, 65)]);

    state.create_pane().unwrap();
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
    state.create_pane().unwrap();
    let id = state.panes[0].id;
    state.resize_visible_panes(&[(id, 10, 40)]);

    state.close_active();

    assert!(!state.last_content_size.contains_key(&id));
}