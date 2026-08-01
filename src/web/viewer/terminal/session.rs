use super::TerminalHub;
use super::frame::TerminalFrame;
use super::frame::{ClearKeyFacts, ClientMessage, PaneSize};
use super::hub_helpers::Command;
use crate::backend::PaneId;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError};
use std::time::{Duration, Instant};

pub(super) struct Client {
    pub(super) id: u64,
    pub(super) tx: SyncSender<TerminalFrame>,
    /// A second handle on this client's socket, only ever used to end the
    /// connection when its queue overflows.
    ///
    /// Dropping it from the broadcast list alone stops the frames but leaves the
    /// connection thread parked in its read, so the client goes quiet while
    /// still believing it is attached — a panel frozen mid-session, with no
    /// close for the page's reconnect to fire on. Closing the socket is what
    /// turns that into the disconnect it already claims to be. The same handle
    /// the daemon keeps on an attached client, for the same reason
    /// (`daemon::clients`).
    ///
    /// `None` for a client with no socket of its own: the daemon's bridge reads
    /// this hub from a thread and hands frames on without blocking, so it cannot
    /// fall behind here — the backpressure that matters to it is applied a layer
    /// out, where it does own a socket.
    pub(super) socket: Option<std::net::TcpStream>,
    /// This connection's registration with the session's size ownership. Kept
    /// beside the hub's own id because the two answer different questions: the
    /// id names a connection to *this* repository, the registration names the
    /// screen behind it, which the session tracks across every repository.
    pub(super) connection: u64,
}

impl Client {
    /// End this client's connection, because it stopped keeping up.
    ///
    /// Only for that: a session leaving of its own accord is already tearing its
    /// socket down, and shutting one it no longer owns down is not this side's
    /// business. Errors are ignored — a socket that is already gone is the
    /// outcome this is asking for.
    pub(super) fn cut_off(&self) {
        if let Some(socket) = &self.socket {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// A client's connection to a repository's terminals.
pub struct TerminalSession {
    pub(super) hub: std::sync::Arc<TerminalHub>,
    pub(super) id: u64,
    /// See [`Client::connection`].
    pub(super) connection: u64,
    /// Behind a lock so the session can be shared: the daemon reads frames on
    /// one thread while requests are dispatched from another. Uncontended in
    /// practice — only the reader ever takes it.
    pub(super) rx: std::sync::Mutex<Receiver<TerminalFrame>>,
    /// How many diagnostic notes this client has left this window (see
    /// [`TerminalSession::log_clear_key`]).
    pub(super) reports: std::sync::Mutex<ReportBudget>,
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
                client: self.id,
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
            // Off the worker queue like `claim_size`: it rearranges the panel
            // and never reaches a PTY, so the queue that serializes work against
            // the backend has nothing to offer it — and a backed-up hub must not
            // drop the message, or the client stays laid out one way while every
            // other client is laid out the other.
            ClientMessage::Zoom { pane } => {
                self.hub.set_zoom(pane);
                return;
            }
            // Off the worker queue for the same reason `start` is: it decides
            // who may resize, and a backed-up hub must not drop the message that
            // hands the sizing over — the client would then be a spectator with
            // no way to find out.
            ClientMessage::ClaimSize => {
                self.hub.claim_size(self.connection);
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
            // A note for the log, not an instruction: it reaches no pane and no
            // worker, so it stays here.
            ClientMessage::ClearKeyReport { pane, key } => {
                self.log_clear_key(pane, key);
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

impl TerminalSession {
    /// Write down what the client says produced a `Ctrl+L` it forwarded.
    ///
    /// Rate limited and sanitized because this is a client talking: the socket is
    /// authenticated, but a page that can be scripted from a browser extension is
    /// exactly the suspect here, and it must not be able to write the log at will
    /// or put arbitrary text in it.
    fn log_clear_key(&self, pane: PaneId, key: Option<ClearKeyFacts>) {
        if !self
            .reports
            .lock()
            .expect("terminal session report budget poisoned")
            .allow(Instant::now())
        {
            return;
        }
        match key {
            Some(facts) => tracing::info!(
                pane,
                client = self.id,
                trusted = facts.trusted,
                repeat = facts.repeat,
                code = %sanitized_code(&facts.code),
                since_ms = facts.since_ms,
                "viewer: a client reports the key event behind a screen-clearing byte"
            ),
            None => tracing::info!(
                pane,
                client = self.id,
                "viewer: a client reports a screen-clearing byte with no key event behind it"
            ),
        }
    }
}

/// Reports one client may log per window. Generous next to a burst worth
/// investigating (tens of events over seconds), small enough that a page cannot
/// fill the disk with them.
const REPORTS_PER_WINDOW: u32 = 120;
const REPORT_WINDOW: Duration = Duration::from_secs(60);

/// A fixed window rather than a sliding one: this bounds the log, and the extra
/// state a sliding window costs buys nothing here.
pub(super) struct ReportBudget {
    window_start: Instant,
    spent: u32,
}

impl ReportBudget {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            window_start: now,
            spent: 0,
        }
    }

    pub(super) fn allow(&mut self, now: Instant) -> bool {
        if now.duration_since(self.window_start) >= REPORT_WINDOW {
            self.window_start = now;
            self.spent = 0;
        }
        if self.spent >= REPORTS_PER_WINDOW {
            return false;
        }
        self.spent += 1;
        true
    }
}

/// A `KeyboardEvent.code` reduced to what one can be: ASCII letters and digits,
/// briefly. Anything else is dropped rather than escaped — the field exists to
/// say `KeyL`, and a log line is not the place to find out what else a page
/// might put there.
pub(super) fn sanitized_code(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(MAX_CODE_LEN)
        .collect();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

const MAX_CODE_LEN: usize = 16;

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.hub.disconnect(self.id);
    }
}
