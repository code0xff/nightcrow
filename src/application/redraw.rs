//! Dirty-frame accounting for the attached TUI.
//!
//! Polling remains frequent so terminal output and watcher events are picked
//! up promptly, but an unchanged model does not need another frame. The
//! event loop records state-changing inputs and queue results here, while the
//! two clocks record visual changes that happen without an input event.

use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RedrawCause {
    Initial,
    Terminal,
    Input,
    Resize,
    Snapshot,
    Tree,
    Git,
    Log,
    AttentionBlink,
    CaretBlink,
    HotFile,
    Session,
    Redraw,
}

#[derive(Debug, Default)]
pub(crate) struct RedrawState {
    dirty: bool,
    screen: Option<(u16, u16)>,
    attention_phase: Option<bool>,
    caret_phase: Option<bool>,
    hot_observed: bool,
    hot_deadline: Option<SystemTime>,
}

impl RedrawState {
    pub(crate) fn new() -> Self {
        let mut state = Self::default();
        state.request(RedrawCause::Initial);
        state
    }

    pub(crate) fn request(&mut self, _cause: RedrawCause) {
        self.dirty = true;
    }

    pub(crate) fn observe_screen(&mut self, width: u16, height: u16) {
        let screen = (width, height);
        if self.screen != Some(screen) {
            self.screen = Some(screen);
            self.request(RedrawCause::Resize);
        }
    }

    pub(crate) fn observe_attention(&mut self, has_attention: bool, bright: bool) {
        let phase = has_attention.then_some(bright);
        if self.attention_phase != phase {
            self.attention_phase = phase;
            self.request(RedrawCause::AttentionBlink);
        }
    }

    pub(crate) fn observe_caret(&mut self, active: bool, lit: bool) {
        let phase = active.then_some(lit);
        if self.caret_phase != phase {
            self.caret_phase = phase;
            self.request(RedrawCause::CaretBlink);
        }
    }

    /// Schedule the next hot-file style transition without adding a periodic
    /// frame clock. A crossed deadline dirties one frame; the caller then
    /// supplies the following deadline calculated from the same clock. A
    /// newly reachable or earlier deadline after the first observation means
    /// the wall clock moved back across a rendered stage, so repaint once to
    /// synchronize that stage too.
    pub(crate) fn observe_hot_deadline(&mut self, next: Option<SystemTime>, now: SystemTime) {
        let crossed = self.hot_deadline.is_some_and(|deadline| now >= deadline);
        let rolled_back = self.hot_observed
            && match (self.hot_deadline, next) {
                (None, Some(_)) => true,
                (Some(previous), Some(next)) => next < previous,
                _ => false,
            };
        self.hot_observed = true;
        self.hot_deadline = next;
        if crossed || rolled_back {
            self.request(RedrawCause::HotFile);
        }
    }

    pub(crate) fn take(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    #[cfg(test)]
    pub(crate) fn is_dirty(&self) -> bool {
        self.dirty
    }
}

#[cfg(test)]
mod tests {
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

        state.observe_screen(100, 40);
        assert!(state.take());
        state.observe_screen(100, 40);
        assert!(!state.take());
        state.observe_screen(101, 40);
        assert!(state.take());
    }

    #[test]
    fn attention_and_caret_only_repaint_when_their_visible_phase_changes() {
        let mut state = RedrawState::new();
        state.take();

        state.observe_attention(true, true);
        assert!(state.take());
        state.observe_attention(true, true);
        assert!(!state.take());
        state.observe_attention(true, false);
        assert!(state.take());
        state.observe_attention(false, true);
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
}
