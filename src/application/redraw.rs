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
    RepoInputFocus,
    HotFile,
    Session,
    Redraw,
}

#[derive(Debug, Default)]
pub(crate) struct RedrawState {
    dirty: bool,
    screen: Option<(u16, u16)>,
    attention_phase: Option<bool>,
    attention_state: Option<Vec<bool>>,
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

    pub(crate) fn observe_screen(&mut self, width: u16, height: u16) -> bool {
        if width == 0 || height == 0 {
            return false;
        }
        let screen = (width, height);
        let changed = self.screen.is_some_and(|previous| previous != screen);
        if self.screen != Some(screen) {
            self.screen = Some(screen);
            self.request(RedrawCause::Resize);
        }
        changed
    }

    pub(crate) fn observe_attention(&mut self, attention: &[bool], bright: bool) {
        let state_changed = self
            .attention_state
            .as_deref()
            .is_some_and(|previous| previous != attention);
        if self.attention_state.as_deref() != Some(attention) {
            self.attention_state = Some(attention.to_vec());
        }
        let has_attention = attention.iter().any(|attention| *attention);
        let phase = has_attention.then_some(bright);
        let phase_changed = self.attention_phase != phase;
        if phase_changed {
            self.attention_phase = phase;
        }
        if state_changed || phase_changed {
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
#[path = "redraw_tests.rs"]
mod tests;
