use super::TerminalHub;
use super::frame::TerminalFrame;
use super::frame::{ClientMessage, PaneSize};
use super::hub_helpers::Command;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::time::Duration;

pub(super) struct Client {
    pub(super) id: u64,
    pub(super) tx: SyncSender<TerminalFrame>,
}

/// A client's connection to a repository's terminals.
pub struct TerminalSession {
    pub(super) hub: std::sync::Arc<TerminalHub>,
    pub(super) id: u64,
    /// Behind a lock so the session can be shared: the daemon reads frames on
    /// one thread while requests are dispatched from another. Uncontended in
    /// practice — only the reader ever takes it.
    pub(super) rx: std::sync::Mutex<Receiver<TerminalFrame>>,
}

impl TerminalSession {
    /// This session's client id, as the hub stamps it on the panes this session
    /// asked for. The daemon reads it to tell its own client's panes from
    /// another's while relaying (see the daemon's `TerminalBridges`).
    pub fn client_id(&self) -> u64 {
        self.id
    }

    /// Wait up to `timeout` for the next frame to write.
    pub fn next_frame(&self, timeout: Duration) -> Option<TerminalFrame> {
        self.rx
            .lock()
            .expect("terminal session receiver poisoned")
            .recv_timeout(timeout)
            .ok()
    }

    /// Handle a decoded control message from this client.
    pub fn dispatch(&self, message: ClientMessage) {
        let command = match message {
            ClientMessage::Create { rows, cols } => {
                let size = PaneSize { rows, cols }.clamped();
                Command::Create {
                    rows: size.rows,
                    cols: size.cols,
                    client: self.id,
                    command: None,
                }
            }
            ClientMessage::Input { pane, data } => Command::Input {
                pane,
                data: data.into_bytes(),
            },
            ClientMessage::Resize { pane, rows, cols } => {
                let size = PaneSize { rows, cols }.clamped();
                Command::Resize {
                    pane,
                    rows: size.rows,
                    cols: size.cols,
                    client: self.id,
                }
            }
            ClientMessage::Close { pane } => Command::Close { pane },
            ClientMessage::Reorder { order } => Command::Reorder { order },
            ClientMessage::CancelRecovery { pane } => Command::CancelRecovery { pane },
            // Off the worker queue for the same reason `start` is: it decides
            // who may resize, and a backed-up hub must not drop the message that
            // hands the sizing over — the client would then be a spectator with
            // no way to find out.
            ClientMessage::ClaimSize => {
                self.hub.claim_size(self.id);
                return;
            }
            // Handled here rather than on the worker thread: it only queues
            // creates, and routing it through the same queue would let a
            // backed-up hub drop the one message that brings the terminals up.
            ClientMessage::Start { sizes } => {
                let sizes: Vec<PaneSize> = sizes.into_iter().map(PaneSize::clamped).collect();
                self.hub.claim_startup(self.id, &sizes);
                return;
            }
        };
        // Never block the connection thread here. The hub drains this queue
        // from the same thread that writes to a PTY master, and that write
        // blocks forever if the child has stopped reading stdin — a blocking
        // send would then wedge every connection thread for this repository.
        // Dropping a command under that much backpressure is the honest
        // outcome; the client is already far ahead of what the shell can take.
        if let Err(TrySendError::Full(_)) = self.hub.commands.try_send(command) {
            tracing::debug!("viewer: terminal command queue full, dropping");
        }
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.hub.disconnect(self.id);
    }
}
