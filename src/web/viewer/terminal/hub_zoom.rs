//! Which pane fills the terminal panel.
//!
//! **The repository's answer, not each page's.** The same reasoning as the pane
//! order (`hub_layout.rs`): every page attached to a repository shows the same
//! terminals, so "which one is filling the panel" is one question. Keeping it
//! per page instead is what the browser used to do, and it cost the state on
//! every reload — a zoom lived in one `useState` and nothing outside that page
//! had ever heard of it.
//!
//! **An attached TUI is told and ignores it** (`backend/hub.rs`). It has a zoom
//! of its own that answers a different question: it follows the TUI's active
//! pane and takes the body from the diff viewer with it. The panes are shared
//! between the two; what fills a screen is that screen's.
//!
//! **In the hub, and not on disk.** A zoom names a pane, and a pane is a child
//! process of this daemon: restarting it destroys the panes, so there is nothing
//! left for a stored zoom to point at. The panel-level maximize (files vs
//! terminal, `prefs/maximized.rs`) *is* stored, and the difference is exactly
//! this — what it names outlives the process. So a zoom survives a page reload
//! and a TUI restart, which is every case there is a pane to come back to.
//!
//! **A pane appearing or leaving ends it**, which is why the two functions here
//! are called from under the same lock that changes the pane list. A zoom that
//! outlived its pane would leave every client rendering an empty panel, and one
//! that survived a `create` would hide the terminal somebody just asked for.

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
