use super::common::*;
use super::*;
use crate::backend::BackendEvent;
use crossterm::event::MouseButton;

#[test]
fn scroll_active_sends_wheel_notches_to_a_mouse_reporting_pane() {
    // Claude Code's startup mode set. Six lines is two wheel notches.
    let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");

    state.scroll_active(true, 6);

    assert_eq!(
        payloads_after_setup(&state),
        vec![b"\x1b[<64;21;6M\x1b[<64;21;6M".to_vec()]
    );
    assert!(
        state.scroll.is_empty(),
        "a wheel-driven pane must not move the emulator's own view"
    );
}

#[test]
fn scroll_active_rounds_a_partial_notch_up() {
    let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

    // One line still has to move the pane; it must not round down to zero
    // notches and silently do nothing.
    state.scroll_active(false, 1);

    assert_eq!(
        payloads_after_setup(&state),
        vec![b"\x1b[<65;21;6M".to_vec()]
    );
}

#[test]
fn scroll_active_sends_arrow_keys_on_the_alternate_screen() {
    let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1049h");

    state.scroll_active(true, 3);

    assert_eq!(
        payloads_after_setup(&state),
        vec![b"\x1b[A\x1b[A\x1b[A".to_vec()]
    );
}

#[test]
fn scroll_active_uses_application_arrow_keys_when_decckm_is_set() {
    let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1049h\x1b[?1h");

    state.scroll_active(false, 2);

    assert_eq!(payloads_after_setup(&state), vec![b"\x1bOB\x1bOB".to_vec()]);
}

#[test]
fn scroll_active_scrolls_the_emulator_for_a_plain_shell() {
    let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());

    state.scroll_active(true, 3);
    state.sync_scroll();

    assert_eq!(state.scroll.get(&id).copied(), Some(3));
    assert!(
        payloads_after_setup(&state).is_empty(),
        "a shell echoes unbound escape sequences into its prompt, so the \
         scrollback branch must write nothing to the PTY"
    );
}

#[test]
fn scroll_active_down_unwinds_the_emulator_offset_for_a_plain_shell() {
    let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());
    state.scroll_active(true, 3);

    state.scroll_active(false, 3);

    assert!(!state.scroll.contains_key(&id));
    assert!(payloads_after_setup(&state).is_empty());
}

#[test]
fn scroll_active_ignores_a_zero_line_request() {
    let (mut state, _) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

    state.scroll_active(true, 0);

    assert!(payloads_after_setup(&state).is_empty());
}

#[test]
fn scroll_pane_moves_a_non_active_panes_view_immediately() {
    let (mut state, events) = state_with_event_queue();
    state.create_pane().unwrap();
    state.create_pane().unwrap();
    let first = state.panes[0].id;
    state.resize_visible_panes(&[(first, 10, 40)]);
    events.borrow_mut().push(BackendEvent::Output {
        pane: first,
        data: shell_output_past_one_screen(),
    });
    state.poll();
    assert_ne!(
        state.active_pane_id(),
        Some(first),
        "test needs the scrolled pane to be non-active"
    );

    state.scroll_pane(first, true, 3, None);

    assert_eq!(state.scroll.get(&first).copied(), Some(3));
    assert_eq!(
        state.emulators.get(&first).unwrap().scroll_offset(),
        3,
        "the per-frame sync only reaches the active pane, so scroll_pane \
         must apply the offset itself"
    );
}

#[test]
fn click_pane_forwards_sgr_press_and_release_to_a_mouse_reporting_pane() {
    let (mut state, id) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");

    assert!(state.click_pane(id, MouseButton::Left, true, 5, 3));
    assert!(state.click_pane(id, MouseButton::Left, false, 5, 3));

    assert_eq!(
        payloads_after_setup(&state),
        vec![b"\x1b[<0;5;3M".to_vec(), b"\x1b[<0;5;3m".to_vec()]
    );
}

#[test]
fn click_pane_stays_silent_for_a_pane_that_never_claimed_the_mouse() {
    let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());

    assert!(!state.click_pane(id, MouseButton::Left, true, 5, 3));
    assert!(!state.click_pane(id, MouseButton::Right, false, 5, 3));

    assert!(
        payloads_after_setup(&state).is_empty(),
        "a shell echoes unbound escape sequences into its prompt, so an \
         unclaimed click must write nothing to the PTY"
    );
}

#[test]
fn wheel_horizontal_pane_forwards_only_to_a_wheel_reporting_pane() {
    let (mut state, id) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

    state.wheel_horizontal_pane(id, true, 5, 2);
    state.wheel_horizontal_pane(id, false, 5, 2);

    assert_eq!(
        payloads_after_setup(&state),
        vec![b"\x1b[<66;5;2M".to_vec(), b"\x1b[<67;5;2M".to_vec()]
    );
}

#[test]
fn wheel_horizontal_pane_stays_silent_for_a_plain_shell() {
    let (mut state, id) = state_with_pane_in_modes(&shell_output_past_one_screen());

    state.wheel_horizontal_pane(id, true, 5, 2);

    assert!(
        payloads_after_setup(&state).is_empty(),
        "horizontal wheel has no scrollback fallback, so an unclaimed \
         notch must write nothing to the PTY"
    );
}

#[test]
fn scroll_pane_reports_the_pointer_cell_when_given_one() {
    let (mut state, id) = state_with_pane_in_modes(b"\x1b[?1000h\x1b[?1006h");

    state.scroll_pane(id, true, 3, Some((5, 2)));

    assert_eq!(
        payloads_after_setup(&state),
        vec![b"\x1b[<64;5;2M".to_vec()]
    );
}