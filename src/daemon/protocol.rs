//! What an attaching client and the daemon say to each other.
//!
//! Carried as JSON in [`FrameKind::Control`](super::frame::FrameKind::Control)
//! frames. Both sides ship in one binary, so this is not a compatibility
//! surface to negotiate — a client and daemon of different versions cannot meet
//! except by running two builds at once, which the version in [`Hello`] reports
//! rather than tries to bridge.

use crate::backend::PaneId;
use crate::web::viewer::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage,
};
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
    /// Ask for the current repository set.
    ListRepos,
    /// Open a repository, or focus it if it is already open.
    OpenRepo { path: String },
    /// Close the repository with this id.
    CloseRepo { repo: String },
    /// Put this repository in front, for the whole session.
    ///
    /// Which project is active is shared, so switching tabs is a request rather
    /// than a local move — every client follows the answer. What stays local is
    /// everything inside a project: the view mode, the cursor, the scroll.
    FocusRepo { repo: String },
    /// Put the repositories in this order.
    ReorderRepos { order: Vec<String> },
    /// Paint the session in this accent, for every client and the browser.
    ///
    /// An index into the accent cycle rather than a "next" step: two clients
    /// cycling at once would otherwise each advance from what they last saw and
    /// land somewhere neither asked for. An index past the end wraps, so a
    /// client never has to know the cycle's length to stay in it.
    SetAccent { accent: usize },
    /// Re-read `config.toml` and apply the tables the session owns.
    ///
    /// Carries nothing: the file is the request. Sending its contents instead
    /// would let a client reconfigure the session from something it made up,
    /// where this way the daemon only ever acts on a file on its own disk that
    /// the user wrote.
    ReloadConfig,
    /// Act on one repository's terminals.
    ///
    /// Carries the hub's own message rather than a parallel set: the browser
    /// and an attached terminal ask for exactly the same things, and two
    /// definitions of "create a pane" would drift. The repository has to ride
    /// along because one socket multiplexes every open repository, where the
    /// browser opens a connection per repository and needs no tag.
    Terminal {
        repo: String,
        message: HubClientMessage,
    },
    /// Ask the daemon to stop. Sent by `nightcrow stop`.
    ///
    /// The daemon runs the same shutdown sequence as SIGINT/SIGTERM — reaping
    /// every child shell — and then closes the connection. No reply is sent;
    /// the connection closing is the acknowledgment.
    Shutdown,
}

/// A message from the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Answer to [`ClientMessage::Hello`], naming the daemon's build.
    Hello {
        version: String,
        /// The id this connection is known by, so the client can tell a pane it
        /// asked for from one that arrived because someone else did. Panes are
        /// created by request and reported to everybody, so without an identity
        /// a client cannot tell the two apart — and would move its focus onto
        /// whatever another client just opened.
        client: u64,
    },
    /// The repository set, sent in answer to a list, open, close, or reorder.
    ///
    /// Every mutation answers with the whole set rather than a delta: the set
    /// is small, bounded by `MAX_PROJECTS`, and another client may have changed
    /// it in between — a delta applied to a stale list silently diverges.
    Repos {
        repos: Vec<RepoSummary>,
        /// The repository the session is focused on, which every client puts in
        /// front. `None` when nothing has been focused yet, in which case a
        /// client keeps whichever tab it is on.
        ///
        /// Carried with the set rather than announced separately because the two
        /// change together — opening a repository focuses it — and a client that
        /// learned them one at a time would render a tab list without knowing
        /// which of them to show.
        #[serde(default)]
        active: Option<String>,
        /// The accent the whole session paints in.
        ///
        /// Rides with the set because the watcher already broadcasts whenever
        /// what it observes differs from what clients were told; a colour picked
        /// in the browser reaches every attached terminal through that same
        /// comparison, with nothing needing to remember to announce it.
        ///
        /// Required, unlike `active`: a default here would be a colour, and a
        /// daemon too old to send one would have this client painting the
        /// session yellow and claiming that was its choice. `None` for `active`
        /// is a state the session really has; there is no such reading of a
        /// missing accent. Two builds of the same version can meet — the
        /// handshake compares version strings — so this is the only thing that
        /// catches it, and a frame it cannot read ends the connection.
        accent: usize,
    },
    /// A request could not be carried out. The connection stays open: a refused
    /// request is an answer, not a protocol violation.
    Error { message: String },
    /// A reload was carried out, described for the person who asked.
    ///
    /// Answered to the asker alone, unlike a change to the served set. Nothing a
    /// reload does is visible in what the other clients are looking at — the
    /// startup list only reaches repositories opened later, and a plugin being
    /// replaced is a child process nobody is watching — so telling them would be
    /// a notice about something they did not do and cannot see. A refusal is
    /// reported the same way as any other, through [`ServerMessage::Error`].
    Reloaded {
        /// One line for the client to show. Built by the session so a browser
        /// toast and a terminal notice say the same thing.
        summary: String,
    },
    /// Something happened to one repository's terminals — a pane was created,
    /// exited, or reordered. Output does not come this way; it travels as
    /// binary frames.
    Terminal {
        repo: String,
        event: HubServerMessage,
    },
}

/// One repository in the served set.
///
/// A narrower view than the browser's `RepoDto`: an attaching client renders
/// with the TUI's own widgets and reads git locally, so it needs the identity
/// and the path, not the display fields the web UI derives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSummary {
    /// Opaque catalog id, stable for the daemon's lifetime.
    pub id: String,
    /// Absolute worktree path.
    pub path: String,
}

/// Bytes a repository's pane produced, and who they belong to.
///
/// Carried in a [`FrameKind::Terminal`](super::frame::FrameKind::Terminal)
/// frame rather than as JSON: PTY output is not guaranteed valid UTF-8 — a
/// multi-byte sequence is routinely split across reads — so encoding it as
/// text would corrupt it before any emulator saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalOutput {
    pub repo: String,
    pub pane: PaneId,
    pub data: Vec<u8>,
}

impl TerminalOutput {
    /// `[repo len][repo][pane id][bytes]`, with the id little-endian to match
    /// the hub's own binary framing.
    pub fn encode(&self) -> Vec<u8> {
        let repo = self.repo.as_bytes();
        // The id space is the catalog's, which hands out short opaque names;
        // anything that does not fit a byte is a bug rather than input.
        let len = u8::try_from(repo.len()).unwrap_or(0);
        let mut out = Vec::with_capacity(1 + repo.len() + 4 + self.data.len());
        out.push(len);
        out.extend_from_slice(&repo[..usize::from(len)]);
        out.extend_from_slice(&self.pane.to_le_bytes());
        out.extend_from_slice(&self.data);
        out
    }

    /// Read one back, or `None` when the frame is too short to hold a header.
    ///
    /// The daemon only encodes and the attaching client only decodes: output
    /// travels one way.
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
