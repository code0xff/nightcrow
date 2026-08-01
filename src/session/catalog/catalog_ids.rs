use crate::session::runtime::RepoRuntime;
use crate::session::terminal::TerminalHub;
use std::collections::HashMap;
use std::sync::Arc;

/// One served repository: its identity, and the runtime streaming its status.
pub struct RepoEntry {
    pub id: String,
    /// Absolute worktree path. Never serialized to the browser, which has no
    /// use for it and no access to it; the attach transport does send it, to
    /// clients reading the same filesystem.
    pub path: String,
    pub name: String,
    pub display_path: String,
    pub runtime: Arc<RepoRuntime>,
    /// This repository's terminals. Independent of the TUI's panes — see
    /// [`crate::session::terminal`].
    pub terminals: Arc<TerminalHub>,
}

impl RepoEntry {
    pub fn info(&self) -> RepoInfo {
        RepoInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            display_path: self.display_path.clone(),
        }
    }
}

/// Repository identity safe to expose to a client. Absolute paths stay on
/// [`RepoEntry`] and are projected separately for attached local clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub id: String,
    pub name: String,
    pub display_path: String,
}

/// Hands out ids and never reuses one, so a path that leaves and comes back
/// keeps the identity a client already knows.
#[derive(Default)]
pub(super) struct IdAssigner {
    next: u64,
    by_path: HashMap<String, String>,
}

impl IdAssigner {
    pub(super) fn id_for(&mut self, path: &str) -> String {
        if let Some(existing) = self.by_path.get(path) {
            return existing.clone();
        }
        self.next += 1;
        let id = format!("r{}", self.next);
        self.by_path.insert(path.to_string(), id.clone());
        id
    }
}

/// Result of [`crate::session::catalog::Catalog::add_path`].
pub enum AddOutcome {
    /// The repository is now served — newly added, or already present.
    Added(RepoInfo),
    /// The served set is already at its ceiling; nothing was added.
    TooMany,
}
