//! A terminal backend whose panes live in the daemon's session.
//!
//! [`PtyBackend`](super::PtyBackend) owns its children; this owns nothing and
//! asks. One per repository, all sharing the connection the client attached
//! with, so the panes it reports are the same ones the browser is looking at.
//!
//! A pane arrives as an event instead of a return value, because its id comes
//! from where the PTY actually lives. Its size is not this client's to assume.
//! VT emulation still happens here: the bytes are raw either way, so
//! `PaneEmulator` reads them from a socket exactly as it read them from a PTY.

use super::{BackendEvent, PaneId, ResizeOutcome, TerminalBackend};
use crate::daemon::terminal_link::{TerminalLink, TerminalMessage};
use crate::session::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage,
};
use anyhow::{Result, bail};

/// The panes of one repository in the daemon's session.
pub struct HubBackend {
    link: TerminalLink,
}

impl HubBackend {
    pub fn new(link: TerminalLink) -> Self {
        Self { link }
    }

    /// Take the daemon up on its offer to size the startup terminals.
    ///
    /// Answered with no sizes at all, because this client has measured nothing:
    /// the offer arrives on attach, before the first frame has laid out a single
    /// pane. The hub opens them at its own default and the first layout corrects
    /// it.
    fn size_startup_panes(&self) {
        if let Err(err) = self
            .link
            .send(HubClientMessage::Start { sizes: Vec::new() })
        {
            tracing::warn!(%err, "could not answer the daemon's startup terminals");
        }
    }
}

impl TerminalBackend for HubBackend {
    /// `command` is refused: a pane in a shared session is a bare shell.
    ///
    /// The session's configured commands are run once by the daemon, for every
    /// client, so there is no request here that would carry one — and the hub
    /// deliberately gives a client no way to ask for a pane running arbitrary
    /// text.
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<()> {
        if let Some(command) = command {
            bail!("a shared session cannot open a pane running `{command}`");
        }
        self.link.send(HubClientMessage::Create { rows, cols })
    }

    fn destroy_pane(&mut self, id: PaneId) {
        if let Err(err) = self.link.send(HubClientMessage::Close { pane: id }) {
            tracing::warn!(%err, pane = id, "could not ask the daemon to close a pane");
        }
    }

    /// Everything a client sends a pane is UTF-8 by construction — key
    /// encodings, pasted text, and the emulator's own replies to terminal
    /// queries are all either ASCII control bytes or encoded characters — so the
    /// text-shaped `input` message the browser already uses carries them
    /// losslessly. Anything else is a bug on this side rather than something to
    /// widen the wire format for, and is reported as one.
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()> {
        let Ok(data) = String::from_utf8(data.to_vec()) else {
            // The bytes themselves stay out of it: this is what the user typed,
            // and the caller logs the error. The length is what identifies which
            // encoding produced it.
            bail!("pane {id} input is not valid UTF-8 ({} bytes)", data.len());
        };
        self.link.send(HubClientMessage::Input { pane: id, data })
    }

    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) -> Result<ResizeOutcome> {
        self.link.send(HubClientMessage::Resize {
            pane: id,
            rows,
            cols,
        })?;
        Ok(ResizeOutcome::Pending)
    }

    fn reorder(&mut self, order: &[PaneId]) {
        if let Err(err) = self.link.send(HubClientMessage::Reorder {
            order: order.to_vec(),
        }) {
            tracing::warn!(%err, "could not ask the session to reorder its panes");
        }
    }

    fn claim_size(&mut self) {
        if let Err(err) = self.link.send(HubClientMessage::ClaimSize) {
            tracing::warn!(%err, "could not ask the session for the pane sizing");
        }
    }

    fn cancel_recovery(&mut self, id: PaneId) {
        if let Err(err) = self
            .link
            .send(HubClientMessage::CancelRecovery { pane: id })
        {
            tracing::warn!(%err, pane = id, "could not cancel a pane's recovery");
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        for message in self.link.drain() {
            match message {
                TerminalMessage::Output { pane, data } => {
                    events.push(BackendEvent::Output { pane, data })
                }
                TerminalMessage::Event(HubServerMessage::Created {
                    pane,
                    rows,
                    cols,
                    client,
                    title,
                }) => events.push(BackendEvent::Created {
                    pane,
                    rows,
                    cols,
                    requested: client == Some(self.link.client_id()),
                    title,
                }),
                TerminalMessage::Event(HubServerMessage::Exited { pane }) => {
                    events.push(BackendEvent::Exited { pane })
                }
                TerminalMessage::Event(HubServerMessage::Resized { pane, rows, cols }) => {
                    events.push(BackendEvent::Resized { pane, rows, cols })
                }
                TerminalMessage::Event(HubServerMessage::SizeOwner { owned }) => {
                    events.push(BackendEvent::SizeOwnership { owned })
                }
                TerminalMessage::Event(HubServerMessage::Pending { .. }) => {
                    self.size_startup_panes()
                }
                TerminalMessage::Event(HubServerMessage::Reordered { order }) => {
                    events.push(BackendEvent::Reordered { order })
                }
                TerminalMessage::Event(HubServerMessage::Attention { pane }) => {
                    events.push(BackendEvent::Attention { pane })
                }
                TerminalMessage::Event(HubServerMessage::Recovery {
                    pane,
                    state,
                    detail,
                    deadline_epoch,
                    attempt,
                }) => events.push(BackendEvent::Recovery {
                    pane,
                    state,
                    detail,
                    deadline_epoch,
                    attempt,
                }),
                // An attached client already knows who it is — the daemon told
                // it when it subscribed, and `rewrite_requester` restates every
                // `created` in that id space before it gets here. This names the
                // browser-side hub connection, which is one hop in.
                TerminalMessage::Event(HubServerMessage::Hello { .. }) => {}
                // Deliberately dropped: a browser's zoom is not this client's.
                // The TUI has a zoom of its own that means something else — it
                // follows *its* active pane and takes the body from the diff
                // viewer with it (`TerminalFullscreen::Zoom`), so letting a page
                // drive it would let someone at a browser hide a panel here.
                // The panes are shared; what fills a screen is each screen's.
                TerminalMessage::Event(HubServerMessage::Zoomed { .. }) => {}
                // Refusals do not come this way — they are not about a pane, so
                // the client keeps them on the queue that reaches its notices.
                TerminalMessage::Event(HubServerMessage::Error { message }) => {
                    tracing::warn!(%message, "unexpected terminal refusal on a pane inbox");
                }
            }
        }
        events
    }
}

#[cfg(test)]
#[path = "hub_tests.rs"]
mod tests;
