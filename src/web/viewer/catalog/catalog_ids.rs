use crate::web::viewer::dto::RepoDto;
use crate::web::viewer::runtime::RepoRuntime;
use crate::web::viewer::terminal::TerminalHub;
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
    /// [`crate::web::viewer::terminal`].
    pub terminals: Arc<TerminalHub>,
}

impl RepoEntry {
    pub fn to_dto(&self) -> RepoDto {
        RepoDto {
            id: self.id.clone(),
            name: self.name.clone(),
            display_path: self.display_path.clone(),
        }
    }
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

/// Result of [`crate::web::viewer::catalog::Catalog::add_path`].
pub enum AddOutcome {
    /// The repository is now served — newly added, or already present.
    Added(RepoDto),
    /// The served set is already at its ceiling; nothing was added.
    TooMany,
}
