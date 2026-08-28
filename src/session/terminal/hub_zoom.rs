//! Which pane fills the terminal panel — the repository's answer, not each
//! page's, because every page attached to a repository shows the same terminals
//! and per-page state was lost on every reload. An attached TUI is told and
//! ignores it: it has a zoom of its own that follows the TUI's active pane and
//! takes the body from the diff viewer with it.
//!
//! Kept in the hub, not on disk: a zoom names a pane, a pane is a child process
//! of this daemon, so a restart destroys what a stored zoom would point at
//! (the panel-level maximize in `prefs/maximized.rs` *is* stored, and the
//! difference is exactly this). A pane appearing or leaving ends it, which is
//! why the two functions here run under the same lock that changes the pane
//! list — a zoom that outlived its pane leaves every client an empty panel, and
//! one that survived a `create` hides the terminal somebody just asked for.

use super::TerminalHub;
use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{Shared, broadcast_locked};
use crate::backend::PaneId;

impl TerminalHub {
    /// Zoom `pane`, or un-zoom with `None`, and tell every client.
    ///
    /// Honoured from whoever asks, like input and unlike a resize: a zoom
    /// rearranges a panel, it does not reach a PTY, so there is nothing here for
    /// the sizing owner to arbitrate.
    ///
    /// A pane that is not live is ignored rather than errored — a client racing
    /// a pane exit is normal — and so is a zoom that changes nothing, which
    /// would otherwise make every client relayout its grid for no news.
    pub(super) fn set_zoom(&self, pane: Option<PaneId>) {
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(id) = pane
            && !state.panes.iter().any(|p| p.id == id)
        {
            return;
        }
        if state.zoomed == pane {
            return;
        }
        state.zoomed = pane;
        announce_zoom(&mut state, pane);
    }
}

/// Drop the zoom and tell every client, if there is one to drop.
///
/// Takes the already-locked state: both callers are mid-way through changing the
/// pane list and must not let a client see the new list under the old zoom.
pub(super) fn clear_zoom_locked(state: &mut Shared) {
    if state.zoomed.take().is_none() {
        return;
    }
    announce_zoom(state, None);
}

fn announce_zoom(state: &mut Shared, pane: Option<PaneId>) {
    if let Ok(json) = serde_json::to_string(&ServerMessage::Zoomed { pane }) {
        broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
    }
}
