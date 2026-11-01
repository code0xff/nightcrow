//! Draining backend events into pane emulators and reporting frame impact.
//!
//! Every event is applied, including output for a pane the split window does
//! not show: the emulator has to stay current for the moment the pane is
//! revealed. Only the report distinguishes that from a change the next frame
//! would actually show.

use crate::backend::{BackendEvent, PaneId};
use crate::runtime::terminal::TerminalState;

/// What one backend drain produced for the renderer.
pub(crate) struct PollOutcome {
    /// Panes whose process ended during this drain.
    pub exited: Vec<PaneId>,
    /// Whether the next frame would differ. Output for a pane outside the
    /// visible split window only reaches that pane's emulator, so it leaves
    /// this false.
    pub frame_changed: bool,
}

impl TerminalState {
    /// Drain pending backend events into pane emulators and pane metadata.
    /// Returns the pane ids the backend signalled as exited so the caller
    /// can run cross-cutting cleanup (focus redirect, fullscreen reset).
    #[cfg(test)]
    pub fn poll(&mut self) -> Vec<PaneId> {
        self.poll_at(std::time::Instant::now())
    }

    #[cfg(test)]
    pub(crate) fn poll_at(&mut self, now: std::time::Instant) -> Vec<PaneId> {
        self.poll_at_with_activity(now).exited
    }

    /// Drain terminal events and report whether anything can affect the next
    /// rendered frame. The ordinary `poll` API remains exit-only for callers
    /// that need just lifecycle cleanup; the TUI also needs visible output,
    /// title, resize, lifecycle, attention, and delayed synchronized-update
    /// activity.
    pub(crate) fn poll_with_activity(&mut self) -> PollOutcome {
        self.poll_at_with_activity(std::time::Instant::now())
    }

    pub(crate) fn poll_at_with_activity(&mut self, now: std::time::Instant) -> PollOutcome {
        let mut exited = Vec::new();
        let mut frame_changed = false;
        let attention_before = self.unread_attention;
        let events: Vec<BackendEvent> = self
            .backend
            .as_mut()
            .map(|b| b.drain_events())
            .unwrap_or_default();

        for event in events {
            match event {
                BackendEvent::Created {
                    pane,
                    rows,
                    cols,
                    requested,
                    title,
                } => frame_changed |= self.adopt_pane(pane, rows, cols, requested, title),
                BackendEvent::Output { pane, data } => {
                    frame_changed |= self.apply_output(pane, &data, now);
                }
                BackendEvent::ReplayOutput { pane, data } => {
                    let Some(emulator) = self.emulators.get_mut(&pane) else {
                        continue;
                    };
                    emulator.suppress_pty_replies(true);
                    self.replaying_panes.insert(pane);
                    frame_changed |= self.apply_output(pane, &data, now);
                }
                BackendEvent::ReplayComplete => {
                    let panes: Vec<PaneId> = self.replaying_panes.drain().collect();
                    for pane in panes {
                        let visible = self.pane_is_visible(pane);
                        let Some(emulator) = self.emulators.get_mut(&pane) else {
                            continue;
                        };
                        let events = emulator.settle_sync();
                        let title_changed = self.apply_emulator_events(pane, events, now);
                        frame_changed |= visible || title_changed;
                        if let Some(emulator) = self.emulators.get_mut(&pane) {
                            emulator.suppress_pty_replies(false);
                        }
                    }
                }
                // The session set this pane to a size, which may not be the one
                // this client asked for — or any it asked for. The emulator has
                // to wrap where the child does, so it follows.
                BackendEvent::Resized { pane, rows, cols } => {
                    let visible = self.pane_is_visible(pane);
                    let resized = self.confirm_resize(pane, rows, cols);
                    frame_changed |= visible && resized;
                }
                // Only for a pane this client holds, like `Exited`: a marker
                // for a pane that is not on any of this client's tabs would
                // have nothing to point at.
                BackendEvent::Attention { pane } => {
                    if self.panes.iter().any(|p| p.id == pane) {
                        self.raise_attention();
                    }
                }
                BackendEvent::Reordered { order } => {
                    frame_changed |= self.apply_order(&order);
                }
                BackendEvent::Recovery {
                    pane,
                    state,
                    detail,
                    deadline_epoch,
                    attempt,
                } => {
                    self.apply_recovery(pane, state, detail, deadline_epoch, attempt);
                    frame_changed = true;
                }
                BackendEvent::SizeOwnership { owned } => {
                    let changed = owned != self.owns_size;
                    // Gaining it means the panes are this client's layout to
                    // set, and they are currently at someone else's sizes — so
                    // forget what was applied and let the next frame fit them.
                    if owned && !self.owns_size {
                        self.last_content_size.clear();
                    } else if !owned && self.owns_size {
                        self.last_content_size = self.confirmed_content_size.clone();
                    }
                    if owned != self.owns_size {
                        self.pending_content_size.clear();
                    }
                    self.owns_size = owned;
                    frame_changed |= changed;
                }
                BackendEvent::Exited { pane } => {
                    // Only for a pane this client still has. An exit can arrive
                    // for one already gone — reported twice, or for a pane
                    // another client closed and this one never adopted — and
                    // acting on it would send a close request for a pane that no
                    // longer exists and clamp focus over nothing.
                    if !self.panes.iter().any(|p| p.id == pane) {
                        continue;
                    }
                    self.raise_attention();
                    // Told to the backend as well as forgotten here: a local
                    // `PtyBackend` leaves pane removal to its caller (see its
                    // `drain_events`), so this is what releases the PTY.
                    if let Some(backend) = &mut self.backend {
                        backend.destroy_pane(pane);
                    }
                    tracing::info!(pane, "terminal pane closed");
                    self.remove_pane_state(pane);
                    self.panes.retain(|p| p.id != pane);
                    exited.push(pane);
                    frame_changed = true;
                }
            }
        }
        // After the drain, so an update this tick's own output closed is never
        // cut short by the clock.
        let settled_sync = self.settle_sync_updates(now);
        let settled_title = self.settle_title_attention(now);
        PollOutcome {
            exited,
            frame_changed: frame_changed
                || settled_sync
                || settled_title
                || self.unread_attention != attention_before,
        }
    }

    /// Feed one chunk of output to `pane`'s emulator and report whether the
    /// frame would differ. Hidden-pane bytes still reach the emulator; they
    /// simply have nothing on screen to update.
    fn apply_output(&mut self, pane: PaneId, data: &[u8], now: std::time::Instant) -> bool {
        let visible = self.pane_is_visible(pane);
        let Some(emulator) = self.emulators.get_mut(&pane) else {
            return false;
        };
        let events = emulator.process(data);
        let title_changed = self.apply_emulator_events(pane, events, now);
        visible || title_changed
    }
}
