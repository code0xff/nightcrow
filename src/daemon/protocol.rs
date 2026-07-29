//! What an attaching client and the daemon say to each other.
//!
//! Carried as JSON in [`FrameKind::Control`](super::frame::FrameKind::Control)
//! frames. Both sides ship in one binary, so this is not a compatibility
//! surface to negotiate — a client and daemon of different versions cannot meet
//! except by running two builds at once, which the version in [`Hello`] reports
//! rather than tries to bridge.

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
    /// Put the repositories in this order.
    ReorderRepos { order: Vec<String> },
}

/// A message from the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Answer to [`ClientMessage::Hello`], naming the daemon's build.
    Hello { version: String },
    /// The repository set, sent in answer to a list, open, close, or reorder.
    ///
    /// Every mutation answers with the whole set rather than a delta: the set
    /// is small, bounded by `MAX_PROJECTS`, and another client may have changed
    /// it in between — a delta applied to a stale list silently diverges.
    Repos { repos: Vec<RepoSummary> },
    /// A request could not be carried out. The connection stays open: a refused
    /// request is an answer, not a protocol violation.
    Error { message: String },
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

/// This build's version, reported in the hello exchange.
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
