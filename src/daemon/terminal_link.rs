//! Splitting one attach socket's terminal traffic per repository.
//!
//! A client renders a tab per repository and each tab drives its own panes, but
//! they all share one connection: the daemon multiplexes every open repository
//! over it. So the connection's reader thread files what arrives under the
//! repository it names, and each repository's backend drains only its own
//! inbox — on the render tick, where it must never wait on a socket.

use super::protocol::ClientMessage;
mod coalescing;
use super::wire::{Writer, send};
use crate::backend::PaneId;
use crate::session::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage,
};
use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::sync::{Arc, Mutex};

/// Terminal bytes one attach connection may have waiting in memory.
///
/// The daemon's terminal queue can legally replay this much at once (256
/// one-MiB frames). Keeping the same ceiling here means a valid largest replay
/// can land, while a client that cannot keep up eventually reconnects instead
/// of growing without bound.
pub(crate) const TERMINAL_INBOX_BYTES: usize = 256 * 1024 * 1024;

/// Work one repository may hand to its emulator in one render tick.
const TERMINAL_DRAIN_MESSAGES: usize = 64;
const TERMINAL_DRAIN_BYTES: usize = 256 * 1024;
/// Messages one attach connection may retain, including control-only traffic.
const TERMINAL_INBOX_MESSAGES: usize = 4096;

/// One thing the daemon said about a repository's terminals.
#[derive(Debug)]
pub(crate) enum TerminalMessage {
    /// A pane was created, exited, reordered, or is waiting to be sized.
    Event(HubServerMessage),
    /// Raw bytes a pane produced.
    Output { pane: PaneId, data: Vec<u8> },
}

impl TerminalMessage {
    fn output_bytes(&self) -> usize {
        match self {
            Self::Output { data, .. } => data.len(),
            Self::Event(_) => 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct TerminalInboxOverflow {
    queued: usize,
    incoming: usize,
    limit: usize,
    messages: usize,
    message_limit: usize,
}

impl fmt::Display for TerminalInboxOverflow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "terminal inbox capacity exceeded: {} queued output bytes plus {} incoming \
             (limit {}), {} messages (limit {})",
            self.queued, self.incoming, self.limit, self.messages, self.message_limit
        )
    }
}

impl std::error::Error for TerminalInboxOverflow {}

#[derive(Debug, Default)]
struct RouterState {
    inboxes: HashMap<String, VecDeque<TerminalMessage>>,
    queued_output_bytes: usize,
    queued_messages: usize,
    overflowed: bool,
}

/// Per-repository inboxes, filled by the connection's reader thread and drained
/// by each repository's backend.
#[derive(Debug)]
pub(crate) struct TerminalRouter {
    state: Mutex<RouterState>,
    byte_limit: usize,
    message_limit: usize,
}

impl Default for TerminalRouter {
    fn default() -> Self {
        Self {
            state: Mutex::new(RouterState::default()),
            byte_limit: TERMINAL_INBOX_BYTES,
            message_limit: TERMINAL_INBOX_MESSAGES,
        }
    }
}

impl TerminalRouter {
    /// File one message under its repository. The inbox is created on arrival
    /// rather than when a backend registers: the daemon subscribes a client to
    /// every open repository the moment it connects, so a pane and its
    /// scrollback can be on the wire before the client has been told the
    /// repository exists — and the replay happens only once, so dropping those
    /// would orphan panes.
    ///
    /// Bytes are never discarded from a live stream. If accepting the whole
    /// message would cross the connection-wide ceiling, the router is poisoned
    /// and the socket reader ends the connection. A later attach is replayed a
    /// coherent stream by the session hub; continuing after a partial drop
    /// could never be repaired.
    ///
    /// Adjacent output frames for one pane coalesce within that repository
    /// while their combined payload fits one drain chunk. A single oversized
    /// incoming frame remains a single message so replay cannot wedge here.
    pub(crate) fn deliver(
        &self,
        repo: &str,
        message: TerminalMessage,
    ) -> Result<(), TerminalInboxOverflow> {
        let incoming = message.output_bytes();
        let mut state = self.state.lock().expect("terminal inboxes poisoned");
        // A PTY read is not a protocol boundary: ConPTY can split one burst
        // into thousands of adjacent one-byte reads. Keep one queue entry per
        // contiguous pane run within this repository, bounded to one drain
        // chunk. This keeps the message ceiling protecting control traffic
        // and pane boundaries without turning read granularity into a
        // disconnect condition.
        let coalesces = coalescing::fits(
            state.inboxes.get(repo).and_then(VecDeque::back),
            &message,
            TERMINAL_DRAIN_BYTES,
        );
        let bytes_fit = state
            .queued_output_bytes
            .checked_add(incoming)
            .is_some_and(|total| total <= self.byte_limit);
        let messages_fit = coalesces || state.queued_messages < self.message_limit;
        if state.overflowed || !bytes_fit || !messages_fit {
            state.overflowed = true;
            return Err(TerminalInboxOverflow {
                queued: state.queued_output_bytes,
                incoming,
                limit: self.byte_limit,
                messages: state.queued_messages,
                message_limit: self.message_limit,
            });
        }
        state.queued_output_bytes += incoming;
        if !coalesces {
            state.queued_messages += 1;
        }
        let inbox = state.inboxes.entry(repo.to_string()).or_default();
        if coalesces {
            coalescing::append_to_fitting_tail(inbox, message);
        } else {
            inbox.push_back(message);
        }
        Ok(())
    }

    /// A bounded FIFO prefix filed for `repo` since the last drain.
    ///
    /// The byte allowance is soft for the first message: replay frames can be
    /// larger than it, and refusing to take the head would wedge the queue.
    /// Message count also bounds control-only traffic. Leaving the remainder
    /// for the next render tick keeps one loud repository from monopolising the
    /// UI while preserving output-before-exit order.
    pub(crate) fn drain(&self, repo: &str) -> Vec<TerminalMessage> {
        let mut state = self.state.lock().expect("terminal inboxes poisoned");
        let Some(inbox) = state.inboxes.get_mut(repo) else {
            return Vec::new();
        };
        let mut drained = Vec::new();
        let mut output_bytes = 0usize;
        while drained.len() < TERMINAL_DRAIN_MESSAGES {
            let Some(next) = inbox.front() else { break };
            let next_bytes = next.output_bytes();
            if !drained.is_empty() && output_bytes.saturating_add(next_bytes) > TERMINAL_DRAIN_BYTES
            {
                break;
            }
            let message = inbox.pop_front().expect("front was present");
            output_bytes += next_bytes;
            drained.push(message);
        }
        state.queued_output_bytes -= output_bytes;
        state.queued_messages -= drained.len();
        drained
    }

    /// Forget the inboxes of repositories that are no longer open, including
    /// any that were filed for a repository this client never got a tab for.
    pub(crate) fn retain(&self, open: &[String]) {
        let mut state = self.state.lock().expect("terminal inboxes poisoned");
        let mut removed_bytes = 0usize;
        let mut removed_messages = 0usize;
        state.inboxes.retain(|repo, inbox| {
            let keep = open.iter().any(|id| id == repo);
            if !keep {
                removed_bytes += inbox
                    .iter()
                    .map(TerminalMessage::output_bytes)
                    .sum::<usize>();
                removed_messages += inbox.len();
            }
            keep
        });
        state.queued_output_bytes -= removed_bytes;
        state.queued_messages -= removed_messages;
    }

    #[cfg(test)]
    pub(super) fn with_byte_limit(byte_limit: usize) -> Self {
        Self {
            state: Mutex::new(RouterState::default()),
            byte_limit,
            message_limit: TERMINAL_INBOX_MESSAGES,
        }
    }

    #[cfg(test)]
    fn with_limits(byte_limit: usize, message_limit: usize) -> Self {
        Self {
            state: Mutex::new(RouterState::default()),
            byte_limit,
            message_limit,
        }
    }

    #[cfg(test)]
    fn queued_for_test(&self) -> (usize, usize) {
        let state = self.state.lock().expect("terminal inboxes poisoned");
        (state.queued_output_bytes, state.queued_messages)
    }
}

/// One repository's end of the shared connection.
///
/// Cheap to make and to hold: it is two handles onto the connection plus the
/// repository to tag outgoing requests with.
#[derive(Debug)]
pub(crate) struct TerminalLink {
    repo: String,
    out: Writer,
    router: Arc<TerminalRouter>,
    client: u64,
}

impl TerminalLink {
    pub(crate) fn new(repo: &str, out: Writer, router: Arc<TerminalRouter>, client: u64) -> Self {
        Self {
            repo: repo.to_string(),
            out,
            router,
            client,
        }
    }

    /// Ask this repository's terminals for something. The answer, if there is
    /// one, arrives on the inbox rather than here.
    pub(crate) fn send(&self, message: HubClientMessage) -> Result<()> {
        send(
            &self.out,
            &ClientMessage::Terminal {
                repo: self.repo.clone(),
                message,
            },
        )
    }

    /// Everything the daemon has said about this repository's terminals since
    /// the last drain.
    pub(crate) fn drain(&self) -> Vec<TerminalMessage> {
        self.router.drain(&self.repo)
    }

    /// The id this connection is known by at the daemon, which a new pane names
    /// when this client is the one that asked for it.
    pub(crate) fn client_id(&self) -> u64 {
        self.client
    }
}

#[cfg(test)]
#[path = "terminal_link_tests/mod.rs"]
mod tests;
