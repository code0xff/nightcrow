//! Splitting one attach socket's terminal traffic per repository.
//!
//! A client renders a tab per repository and each tab drives its own panes, but
//! they all share one connection: the daemon multiplexes every open repository
//! over it. So the connection's reader thread files what arrives under the
//! repository it names, and each repository's backend drains only its own
//! inbox — on the render tick, where it must never wait on a socket.

use super::protocol::ClientMessage;
use super::wire::{Writer, send};
use crate::backend::PaneId;
use crate::session::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage,
};
use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

/// One thing the daemon said about a repository's terminals.
#[derive(Debug)]
pub(crate) enum TerminalMessage {
    /// A pane was created, exited, reordered, or is waiting to be sized.
    Event(HubServerMessage),
    /// Raw bytes a pane produced.
    Output { pane: PaneId, data: Vec<u8> },
}

/// Per-repository inboxes, filled by the connection's reader thread and drained
/// by each repository's backend.
#[derive(Debug, Default)]
pub(crate) struct TerminalRouter {
    inboxes: Mutex<HashMap<String, VecDeque<TerminalMessage>>>,
}

impl TerminalRouter {
    /// File one message under its repository. The inbox is created on arrival
    /// rather than when a backend registers: the daemon subscribes a client to
    /// every open repository the moment it connects, so a pane and its
    /// scrollback can be on the wire before the client has been told the
    /// repository exists — and the replay happens only once, so dropping those
    /// would orphan panes.
    ///
    /// Unbounded because dropping bytes corrupts a stream that cannot be
    /// re-read; an inbox nobody drains belongs to a repository this client has
    /// not opened a tab for yet, which is the very next thing it does.
    pub(crate) fn deliver(&self, repo: &str, message: TerminalMessage) {
        self.inboxes
            .lock()
            .expect("terminal inboxes poisoned")
            .entry(repo.to_string())
            .or_default()
            .push_back(message);
    }

    /// Everything filed for `repo` since the last drain.
    pub(crate) fn drain(&self, repo: &str) -> Vec<TerminalMessage> {
        let mut inboxes = self.inboxes.lock().expect("terminal inboxes poisoned");
        match inboxes.get_mut(repo) {
            Some(inbox) => inbox.drain(..).collect(),
            None => Vec::new(),
        }
    }

    /// Forget the inboxes of repositories that are no longer open, including
    /// any that were filed for a repository this client never got a tab for.
    pub(crate) fn retain(&self, open: &[String]) {
        self.inboxes
            .lock()
            .expect("terminal inboxes poisoned")
            .retain(|repo, _| open.iter().any(|id| id == repo));
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
#[path = "terminal_link_tests.rs"]
mod tests;
