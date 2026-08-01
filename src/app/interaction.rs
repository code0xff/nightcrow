use crate::backend::PaneId;
use crossterm::event::{KeyEvent, KeyModifiers, MouseButton};

// Kept free-standing so the empty workspace can label its hints too.
pub(crate) fn leader_label_of(leader: KeyEvent) -> String {
    match leader.code {
        crossterm::event::KeyCode::Char(c) if leader.modifiers.contains(KeyModifiers::CONTROL) => {
            format!("^{}", c.to_ascii_uppercase())
        }
        crossterm::event::KeyCode::Char(c) => c.to_string(),
        _ => "<prefix>".to_string(),
    }
}

pub(crate) struct InteractionState {
    pub(crate) leader: KeyEvent,
    // No timeout: a follow-up key or explicit cancellation resolves it.
    pub(crate) prefix_armed: bool,
    // Arming this clears `prefix_armed`.
    pub(crate) awaiting_swap_target: bool,
    // Releases pair with the pane that received the press, not the pointer.
    pub(crate) pending_mouse_press: Option<(PaneId, MouseButton, u16, u16)>,
    // Mirrors `[mouse] enabled` for hint-bar click affordances.
    pub(crate) mouse_enabled: bool,
}

impl InteractionState {
    pub(crate) fn new(leader: KeyEvent) -> Self {
        Self {
            leader,
            prefix_armed: false,
            awaiting_swap_target: false,
            pending_mouse_press: None,
            mouse_enabled: true,
        }
    }

    // Enhanced keyboard protocols can add modifier bits, so only the exact
    // configured chord is the leader; augmented chords pass through.
    pub(crate) fn is_leader_key(&self, key: KeyEvent) -> bool {
        key.code == self.leader.code && key.modifiers == self.leader.modifiers
    }

    pub(crate) fn begin_swap_target(&mut self) {
        self.prefix_armed = false;
        self.awaiting_swap_target = true;
    }
}
