use super::TerminalHub;
use super::frame::ClientMessage;
use super::frame::TerminalFrame;
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
    pub(super) rx: Receiver<TerminalFrame>,
}

impl TerminalSession {
    /// Wait up to `timeout` for the next frame to write.
    pub fn next_frame(&self, timeout: Duration) -> Option<TerminalFrame> {
        self.rx.recv_timeout(timeout).ok()
    }

    /// Handle a decoded control message from this client.
    pub fn dispatch(&self, message: ClientMessage) {
        let command = match message {
            ClientMessage::Create { rows, cols } => Command::Create {
                rows: rows.max(1),
                cols: cols.max(1),
                client: self.id,
                command: None,
            },
            ClientMessage::Input { pane, data } => Command::Input {
                pane,
                data: data.into_bytes(),
            },
            ClientMessage::Resize { pane, rows, cols } => Command::Resize {
                pane,
                rows: rows.max(1),
                cols: cols.max(1),
            },
            ClientMessage::Close { pane } => Command::Close { pane },
            ClientMessage::Reorder { order } => Command::Reorder { order },
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
