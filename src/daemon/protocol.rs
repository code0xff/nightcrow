//! JSON messages between an attaching client and the daemon, carried in
//! [`FrameKind::Control`](super::frame::FrameKind::Control) frames. Both sides
//! ship in one binary, so version mismatch is reported rather than bridged.

use crate::backend::PaneId;
use crate::session::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage,
};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// A request from an attached client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    /// First message on a connection. The daemon answers with [`ServerMessage::Hello`].
    Hello {
        /// The client's build, so a mismatch is reported rather than acted on.
        version: String,
    },
    ListRepos,
    /// Open a repository, or focus it if it is already open.
    OpenRepo {
        path: String,
    },
    CloseRepo {
        repo: String,
    },
    /// Put this repository in front, for the whole session. Active project is
    /// shared — every client follows the answer. What stays local is everything
    /// inside a project: view mode, cursor, scroll.
    FocusRepo {
        repo: String,
    },
    ReorderRepos {
        order: Vec<String>,
    },
    /// Paint the session in this accent, for every client and the browser.
    ///
    /// An index into the accent cycle rather than a "next" step: two clients
    /// cycling at once would not agree on what "next" means. Wraps past the end.
    SetAccent {
        accent: usize,
    },
    /// Re-read `config.toml` and apply the tables the session owns.
    ///
    /// Carries nothing: the daemon only acts on a file on its own disk that the
    /// user wrote, never on contents a client could have made up.
    ReloadConfig,
    /// Act on one repository's terminals. Carries the hub's own message so the
    /// two definitions of "create a pane" cannot drift; the repository rides
    /// along because one socket multiplexes every open repository.
    Terminal {
        repo: String,
        message: HubClientMessage,
    },
    /// Ask the daemon to stop. Runs the same shutdown sequence as SIGINT/SIGTERM
    /// — reaping every child shell — and then closes the connection. No reply
    /// is sent; the connection closing is the acknowledgment.
    Shutdown,
}

/// A message from the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Answer to [`ClientMessage::Hello`], naming the daemon's build.
    Hello {
        version: String,
        /// This connection's id at the daemon, so the client can tell a pane it
        /// asked for from one that arrived because someone else did. Panes are
        /// created by request and reported to everybody.
        client: u64,
    },
    /// The repository set, sent in answer to a list, open, close, or reorder.
    /// The whole set rather than a delta: another client may have changed it in
    /// between, and a delta applied to a stale list silently diverges.
    Repos {
        repos: Vec<RepoSummary>,
        /// The focused repository, if any. Carried with the set because the
        /// two change together — opening a repository focuses it.
        #[serde(default)]
        active: Option<String>,
        /// The session's accent. Required, unlike `active`: a default would
        /// misattribute an old daemon's silence as this client's choice.
        accent: usize,
    },
    /// A request could not be carried out. The connection stays open: a refused
    /// request is an answer, not a protocol violation.
    Error { message: String },
    /// A reload was carried out, described for the person who asked. Answered
    /// to the asker alone: nothing a reload does is visible in what the other
    /// clients are looking at.
    Reloaded {
        /// One line for the client to show. Built by the session so a browser
        /// toast and a terminal notice say the same thing.
        summary: String,
    },
    /// Something happened to one repository's terminals — a pane was created,
    /// exited, or reordered. Output travels as binary frames, not here.
    Terminal {
        repo: String,
        event: HubServerMessage,
    },
}

/// One repository in the served set. Narrower than the browser's `RepoDto`: an
/// attaching client renders with the TUI's own widgets and reads git locally,
/// so the display fields the web UI derives would be dead weight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSummary {
    /// Opaque catalog id, stable for the daemon's lifetime.
    pub id: String,
    /// Absolute worktree path.
    pub path: String,
}

/// Bytes a repository's pane produced, and who they belong to. Carried in a
/// [`FrameKind::Terminal`](super::frame::FrameKind::Terminal) frame rather than
/// as JSON: PTY output is not guaranteed valid UTF-8 — a multi-byte sequence is
/// routinely split across reads — and a text encoding would corrupt it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalOutput {
    pub repo: String,
    pub pane: PaneId,
    pub data: Vec<u8>,
}

impl TerminalOutput {
    /// `[repo len][repo][pane id][bytes]`, little-endian to match the hub's own
    /// binary framing. Refuses long repository ids rather than truncating the
    /// payload into a frame the receiver would misinterpret.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let repo = self.repo.as_bytes();
        let Ok(len) = u8::try_from(repo.len()) else {
            bail!(
                "terminal output repository id is {} bytes; the protocol limit is {}",
                repo.len(),
                u8::MAX
            );
        };
        let mut out = Vec::with_capacity(1 + repo.len() + 4 + self.data.len());
        out.push(len);
        out.extend_from_slice(repo);
        out.extend_from_slice(&self.pane.to_le_bytes());
        out.extend_from_slice(&self.data);
        Ok(out)
    }

    /// Read one back, or `None` when the header is truncated or its repository
    /// id is not valid UTF-8. Only the attaching client decodes: output travels
    /// one way.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let (&len, rest) = bytes.split_first()?;
        let len = usize::from(len);
        if rest.len() < len + 4 {
            return None;
        }
        let (repo, rest) = rest.split_at(len);
        let (pane, data) = rest.split_at(4);
        Some(Self {
            repo: String::from_utf8(repo.to_vec()).ok()?,
            pane: PaneId::from_le_bytes(pane.try_into().ok()?),
            data: data.to_vec(),
        })
    }
}

/// This build's version, reported in the hello exchange.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
