//! A pane asking for its person: told to this hub's clients, and announced to
//! the session's [`Notices`](crate::session::notice::Notices) listeners.

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::broadcast_locked;
use crate::backend::{PaneId, PaneToken, PtyBackend};
use crate::session::notice::Notice;
use std::sync::mpsc::SyncSender;

impl TerminalHub {
    /// Tell every client `pane` wants a person back, then announce it.
    pub(super) fn broadcast_attention(&self, pane: PaneId, message: Option<String>) {
        let Ok(json) = serde_json::to_string(&ServerMessage::Attention {
            pane,
            message: message.clone(),
        }) else {
            return;
        };
        let mut state = self.state.lock().expect("terminal state poisoned");
        broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
        drop(state);
        self.publish_notice(pane, message);
    }

    /// Announce `pane` to the session's listeners, labelled with its title.
    pub(super) fn publish_notice(&self, pane: PaneId, message: Option<String>) {
        let pane_title = self
            .state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .find(|p| p.id == pane)
            .and_then(|p| p.title.clone());
        self.notices.publish(&Notice {
            repo_path: self.cwd.clone(),
            pane,
            pane_title,
            message,
        });
    }

    /// `nightcrow notify` from inside a pane: answer whether `token` is one of
    /// this hub's live panes, and raise attention there when it is. The token
    /// is the only proof of origin — it reaches a process only through the
    /// environment of the pane it names.
    pub(super) fn notify_by_token(
        &self,
        backend: &PtyBackend,
        token: &PaneToken,
        message: Option<String>,
        reply: &SyncSender<bool>,
    ) {
        let pane = backend
            .pane_for_token(token)
            .filter(|&pane| self.pane_is_live(pane));
        if let Some(pane) = pane {
            tracing::debug!(pane, "viewer: a pane asks for attention");
            self.broadcast_attention(pane, message);
        }
        let _ = reply.try_send(pane.is_some());
    }
}
