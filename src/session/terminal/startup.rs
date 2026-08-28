//! Opening the terminals a repository was configured with.
//!
//! Not on the hub's own initiative: a PTY created before any client has measured
//! its cell is born at a size nobody chose, and correcting it costs the child a
//! full repaint. So the hub offers them and creates them at the size the first
//! client to answer reports — exactly once for its life.

use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::{self, Command, StartupPane};
use super::{DEFAULT_PANE_SIZE, PaneSize, TerminalHub};
use crate::session::limits;
use std::sync::atomic::Ordering;

impl TerminalHub {
    /// Create the startup terminals at the sizes a client measured, if nobody
    /// has claimed them yet.
    ///
    /// The claim is taken here rather than when a client connects because a
    /// client that never answers must not consume it — the next one to connect
    /// is offered the panes again. Two clients answering at once is normal
    /// (both were offered): the first to arrive wins and the second is ignored,
    /// so the panes are created exactly once.
    pub(super) fn claim_startup(&self, client: u64, sizes: &[PaneSize]) {
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        // A bare shell when nothing is configured, matching the TUI's default.
        let configured: Vec<Option<crate::config::StartupCommand>> = if self.startup.is_empty() {
            vec![None]
        } else {
            self.startup.iter().cloned().map(Some).collect()
        };
        let panes: Vec<StartupPane> = configured
            .into_iter()
            .enumerate()
            .map(|(index, configured)| StartupPane {
                // A client that could only measure some cells still gets the
                // rest; they are born at the default and corrected by the
                // first fit.
                size: sizes.get(index).copied().unwrap_or(DEFAULT_PANE_SIZE),
                // The name it was configured under, else the command text: what
                // the operator would recognise the pane by.
                title: configured.as_ref().map(|sc| {
                    sc.name
                        .clone()
                        .unwrap_or_else(|| sc.command.trim().to_string())
                }),
                // The opt-in, carried through so the worker can record it.
                plugin: configured.as_ref().and_then(|sc| sc.plugin.clone()),
                command: configured.map(|sc| sc.command),
            })
            .collect();

        // Hold the free cap slots before the command is even queued: another
        // connection's handler thread can enqueue creates between here and
        // the worker reaching this batch.
        let reserved = {
            let mut state = self.state.lock().expect("terminal state poisoned");
            let free = limits::MAX_PTYS_PER_REPO.saturating_sub(state.panes.len() + state.reserved);
            let take = panes.len().min(free);
            state.reserved += take;
            take
        };

        // One command for the whole set, not one per pane. Sent with
        // `try_send` because a full queue means backpressure the connection
        // thread must not block on — and as a single message that is
        // all-or-nothing, so there is no state where some startup terminals
        // were accepted and the rest were silently lost.
        if self
            .commands
            .try_send(Command::CreateStartup {
                panes,
                client,
                reserved,
            })
            .is_err()
        {
            self.release_reserved(reserved);
            // Hand the claim back and offer again, or the hub would hold
            // `started` with no terminals to show for it. The re-offer matters
            // as much as the release: this client has already cleared its
            // pending state and will not ask again on its own.
            tracing::warn!("viewer: terminal command queue full, startup deferred");
            self.started.store(false, Ordering::Release);
            // To everyone, not just whoever answered. The offer belongs to
            // whichever client replies first, and the one that just did may be
            // gone by now — or another may have answered while the claim was
            // held and had its `start` ignored. Re-offering to only one leaves
            // the rest with no reason to ask again.
            self.broadcast_pending();
        }
    }

    /// Offer the startup terminals to every connected client.
    fn broadcast_pending(&self) {
        let Ok(json) = serde_json::to_string(&ServerMessage::Pending {
            count: self.startup_count(),
        }) else {
            return;
        };
        let mut state = self.state.lock().expect("terminal state poisoned");
        hub_helpers::broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
    }
}
