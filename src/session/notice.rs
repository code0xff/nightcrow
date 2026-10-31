//! Session-wide "a terminal wants you back" announcements.
//!
//! A hub raises one when a plugin asks for attention, when a plugin reports a
//! pane as needing it, or when a program inside a pane runs `nightcrow notify`.
//! The hub's own clients already hear it as an `attention` control message;
//! this registry carries the same event past them to listeners that are not
//! attached to that repository at all — Web Push, which reaches a phone whose
//! browser is closed. The session owns the registry and knows nothing of what
//! listens: a listener is registered by the transport that delivers it.

use crate::backend::PaneId;
use std::sync::{Arc, Mutex};

/// Longest message a notice carries, in characters. A notification shows a
/// line or two; anything longer is cut rather than refused.
pub const NOTICE_MESSAGE_MAX_CHARS: usize = 200;

/// One announcement. Carries labels a person reads, never terminal content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// The hub's working directory: the repository path as the catalog has it.
    pub repo_path: String,
    pub pane: PaneId,
    /// The pane's current title, when it has one.
    pub pane_title: Option<String>,
    /// What the sender said, when it said anything.
    pub message: Option<String>,
}

type Listener = Arc<dyn Fn(&Notice) + Send + Sync>;

/// The listeners for every hub in one session.
#[derive(Default)]
pub struct Notices {
    listeners: Mutex<Vec<Listener>>,
}

impl Notices {
    pub fn subscribe(&self, listener: impl Fn(&Notice) + Send + Sync + 'static) {
        self.listeners
            .lock()
            .expect("notice listeners poisoned")
            .push(Arc::new(listener));
    }

    /// Hand `notice` to every listener. The list is copied out first so a
    /// listener is never called under the lock; each must return promptly
    /// (queue, not send), because the caller is a hub's worker thread.
    pub fn publish(&self, notice: &Notice) {
        let listeners = self
            .listeners
            .lock()
            .expect("notice listeners poisoned")
            .clone();
        for listener in listeners {
            listener(notice);
        }
    }
}

/// `message` trimmed, emptied to `None`, and cut to
/// [`NOTICE_MESSAGE_MAX_CHARS`].
pub fn clean_message(message: Option<&str>) -> Option<String> {
    let trimmed = message?.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(NOTICE_MESSAGE_MAX_CHARS).collect())
}

#[cfg(test)]
#[path = "notice_tests.rs"]
mod tests;
