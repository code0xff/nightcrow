use super::common::*;
use crate::backend::BackendEvent;

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
fn swap_active_with_exchanges_panes_and_follows_focus() {
    let mut state = state_with_fake();
    state.create_pane_with_now(None, Some("A")).unwrap();
    state.create_pane_with_now(None, Some("B")).unwrap();
    state.create_pane_with_now(None, Some("C")).unwrap();
    state.active = 0; // focus pane "A"
    let a_id = state.panes[0].id;
    let c_id = state.panes[2].id;

    assert!(state.swap_active_with_now(2));

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
    state.create_pane_with_now(None, Some("A")).unwrap();
    state.create_pane_with_now(None, Some("B")).unwrap();
    state.active = 0;

    assert!(!state.swap_active_with_now(5));
    assert_eq!(state.active, 0);
    assert_eq!(state.panes[0].title, "A");
    assert_eq!(state.panes[1].title, "B");
}

#[test]
fn swap_active_with_self_is_noop() {
    let mut state = state_with_fake();
    state.create_pane_with_now(None, Some("A")).unwrap();
    state.create_pane_with_now(None, Some("B")).unwrap();
    state.active = 1;

    assert!(!state.swap_active_with_now(1));
    assert_eq!(state.active, 1);
    assert_eq!(state.panes[1].title, "B");
}

#[test]
fn swap_active_with_preserves_per_pane_state() {
    let mut state = state_with_fake();
    state.create_pane_with_now(None, Some("A")).unwrap();
    state.create_pane_with_now(None, Some("B")).unwrap();
    state.active = 0;
    let a_id = state.panes[0].id;
    // Seed scroll/size state keyed by the moving pane's id.
    state.scroll.insert(a_id, 7);
    state.last_content_size.insert(a_id, (10, 40));

    assert!(state.swap_active_with_now(1));

    // Per-pane state is id-keyed, so it survives the reorder unchanged.
    assert_eq!(state.scroll.get(&a_id), Some(&7));
    assert_eq!(state.last_content_size.get(&a_id), Some(&(10, 40)));
    assert_eq!(state.panes[1].id, a_id);
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

#[test]
fn a_swap_is_asked_for_rather_than_applied_on_the_spot() {
    // The order belongs to the session, so it lands when it comes back — for
    // every client at once instead of this one alone.
    let mut state = state_with_fake();
    state.create_pane_with_now(None, Some("A")).unwrap();
    state.create_pane_with_now(None, Some("B")).unwrap();
    state.active = 0;

    assert!(state.swap_active_with(1));

    assert_eq!(state.panes[0].title, "A", "nothing has moved yet");
    state.poll();
    assert_eq!(state.panes[0].title, "B");
    assert_eq!(state.active, 1, "and focus followed the pane it was on");
}

#[test]
fn an_order_from_the_session_reprojects_the_tabs_without_moving_the_focus() {
    // A reorder in the browser reaches every client. The user is still looking
    // at the same pane, which is now somewhere else in the row.
    let mut state = state_with_fake();
    state.create_pane_with_now(None, Some("A")).unwrap();
    state.create_pane_with_now(None, Some("B")).unwrap();
    state.create_pane_with_now(None, Some("C")).unwrap();
    state.active = 1; // looking at "B"
    let b_id = state.panes[1].id;
    let ids: Vec<_> = state.panes.iter().map(|pane| pane.id).collect();

    state.apply_order(&[ids[2], ids[1], ids[0]]);

    assert_eq!(state.panes[0].title, "C");
    assert_eq!(state.panes[2].title, "A");
    assert_eq!(state.panes[state.active].id, b_id);
}

#[test]
fn an_order_naming_panes_this_client_does_not_have_still_applies() {
    // The session can be a beat ahead — a pane another client just opened, or
    // one that exited here first. Neither may lose a pane or drop an id.
    let mut state = state_with_fake();
    state.create_pane_with_now(None, Some("A")).unwrap();
    state.create_pane_with_now(None, Some("B")).unwrap();
    let ids: Vec<_> = state.panes.iter().map(|pane| pane.id).collect();

    // An unknown id in the middle, and "B" left out entirely.
    state.apply_order(&[9999, ids[0]]);

    assert_eq!(state.panes.len(), 2);
    assert_eq!(state.panes[0].id, ids[0], "what was named comes first");
    assert_eq!(
        state.panes[1].id, ids[1],
        "and what was left out keeps its place"
    );
}

#[test]
fn a_close_is_asked_for_rather_than_applied_on_the_spot() {
    // The pane belongs to the session. Removing it here would show it gone while
    // its process kept running — and a close the session never carried out would
    // leave this client unable to see that pane again.
    let mut state = state_with_fake();
    state.create_pane_now().unwrap();
    let pane = state.panes[0].id;

    assert!(state.close_active());

    assert_eq!(state.panes.len(), 1, "nothing has gone yet");
    let exited = state.poll();
    assert_eq!(exited, vec![pane]);
    assert!(state.panes.is_empty());
}

#[test]
fn an_exit_for_a_pane_this_client_does_not_have_is_dropped() {
    // Reported twice, or for a pane another client closed before this one ever
    // adopted it. Acting on it would ask the session to close a pane that is
    // already gone and clamp focus over nothing.
    let (mut state, events) = state_with_event_queue();
    state.create_pane_now().unwrap();
    events
        .borrow_mut()
        .push(crate::backend::BackendEvent::Exited { pane: 9999 });

    assert!(state.poll().is_empty(), "nothing of this client's exited");
    assert_eq!(state.panes.len(), 1);
}

#[test]
fn a_pane_the_session_named_keeps_that_name() {
    // A configured startup terminal is called the same thing in every client:
    // this one did not ask for it and has no title queued for it, so the name
    // has to come with the pane.
    let (mut state, events) = state_with_event_queue();
    events.borrow_mut().push(BackendEvent::Created {
        pane: 1,
        rows: 24,
        cols: 80,
        requested: false,
        title: Some("Claude".into()),
    });

    state.poll();

    assert_eq!(state.panes[0].title, "Claude");
}

#[test]
fn a_pane_nobody_named_falls_back_to_its_position() {
    let (mut state, events) = state_with_event_queue();
    events.borrow_mut().push(BackendEvent::Created {
        pane: 1,
        rows: 24,
        cols: 80,
        requested: false,
        title: None,
    });

    state.poll();

    assert_eq!(state.panes[0].title, "shell 1");
}
