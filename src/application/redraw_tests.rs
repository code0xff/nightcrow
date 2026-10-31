use super::{RedrawCause, RedrawState};
use std::time::SystemTime;

#[test]
fn initial_state_draws_once_then_stays_clean() {
    let mut state = RedrawState::new();

    assert!(state.take());
    assert!(!state.take());
}

#[test]
fn every_external_cause_marks_the_next_frame_dirty() {
    let causes = [
        RedrawCause::Terminal,
        RedrawCause::Input,
        RedrawCause::Resize,
        RedrawCause::Snapshot,
        RedrawCause::Tree,
        RedrawCause::Git,
        RedrawCause::Log,
        RedrawCause::HotFile,
        RedrawCause::RepoInputFocus,
        RedrawCause::Session,
        RedrawCause::Redraw,
    ];

    let mut state = RedrawState::new();
    state.take();
    for cause in causes {
        state.request(cause);
        assert!(state.take(), "{cause:?} must repaint");
        assert!(!state.is_dirty());
    }
}

#[test]
fn screen_change_is_dirty_but_same_size_is_idle() {
    let mut state = RedrawState::new();
    state.take();

    assert!(!state.observe_screen(0, 0));
    assert!(!state.observe_screen(100, 40));
    assert!(state.take());
    assert!(!state.observe_screen(100, 40));
    assert!(!state.take());
    assert!(!state.observe_screen(0, 40));
    assert!(!state.observe_screen(100, 40));
    assert!(!state.take());
    assert!(state.observe_screen(101, 40));
    assert!(state.take());
}

#[test]
fn attention_and_caret_only_repaint_when_their_visible_phase_changes() {
    let mut state = RedrawState::new();
    state.take();

    state.observe_attention(&[true], true);
    assert!(state.take());
    state.observe_attention(&[true], true);
    assert!(!state.take());
    state.observe_attention(&[true], false);
    assert!(state.take());
    state.observe_attention(&[false], true);
    assert!(state.take());

    state.observe_caret(true, true);
    assert!(state.take());
    state.observe_caret(true, true);
    assert!(!state.take());
    state.observe_caret(true, false);
    assert!(state.take());
    state.observe_caret(false, true);
    assert!(state.take());
}

#[test]
fn switching_between_unread_tabs_repaints_the_changed_badge() {
    let mut state = RedrawState::new();
    state.take();
    state.observe_attention(&[true, false], true);
    assert!(state.take());
    state.observe_attention(&[false, true], true);
    assert!(state.take());
    state.observe_attention(&[false, true], true);
    assert!(!state.take());
}

#[test]
fn hot_deadline_dirties_exactly_once_at_each_stage_boundary() {
    let mut state = RedrawState::new();
    state.take();
    let start = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
    let fresh_to_warm = start + std::time::Duration::from_secs(5);
    let warm_to_cool = start + std::time::Duration::from_secs(15);

    state.observe_hot_deadline(Some(fresh_to_warm), start);
    assert!(!state.take());
    state.observe_hot_deadline(Some(warm_to_cool), fresh_to_warm);
    assert!(state.take());
    state.observe_hot_deadline(Some(warm_to_cool), fresh_to_warm);
    assert!(!state.take());
    state.observe_hot_deadline(None, warm_to_cool);
    assert!(state.take());
    state.observe_hot_deadline(None, warm_to_cool);
    assert!(!state.take());
}

#[test]
fn hot_deadline_rollback_from_cool_to_fresh_dirties_exactly_once() {
    let mut state = RedrawState::new();
    state.take();
    let cool_now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(120);
    let rolled_back_now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(100);
    let fresh_to_warm = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(105);

    state.observe_hot_deadline(None, cool_now);
    assert!(!state.take());
    state.observe_hot_deadline(Some(fresh_to_warm), rolled_back_now);
    assert!(state.take(), "cool-to-fresh rollback must repaint");
    state.observe_hot_deadline(Some(fresh_to_warm), rolled_back_now);
    assert!(!state.take(), "the synchronized phase must stay idle");
}

#[test]
fn hot_deadline_rollback_from_warm_to_fresh_dirties_exactly_once() {
    let mut state = RedrawState::new();
    state.take();
    let warm_now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(110);
    let warm_to_cool = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(115);
    let rolled_back_now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(102);
    let fresh_to_warm = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(105);

    state.observe_hot_deadline(Some(warm_to_cool), warm_now);
    assert!(!state.take());
    state.observe_hot_deadline(Some(fresh_to_warm), rolled_back_now);
    assert!(state.take(), "warm-to-fresh rollback must repaint");
    state.observe_hot_deadline(Some(fresh_to_warm), rolled_back_now);
    assert!(!state.take(), "the synchronized phase must stay idle");
}
