use crate::app::{Focus, ViewMode};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub focus: Option<Focus>,
    pub selected_file: Option<String>,
    pub scroll: usize,
    pub active_pane: usize,
    #[serde(default)]
    pub terminal_fullscreen: bool,
    #[serde(default)]
    pub diff_fullscreen: bool,
    #[serde(default)]
    pub list_fullscreen: bool,
    #[serde(default)]
    pub mode: Option<ViewMode>,
    #[serde(default)]
    pub log_selected: usize,
    // No accent here: it is the session's, not one repository's view state, and
    // lives in `viewer.json` (see the boundary in `docs/architecture.md`). An
    // `accent_idx` left over from before is ignored on read rather than
    // migrated — one of several per-repo colours cannot answer what the
    // session's colour is.
    #[serde(default)]
    pub log_drill_down: bool,
    #[serde(default)]
    pub log_file_selected: usize,
    /// Repo-relative path the tree cursor was on (Tree mode only).
    #[serde(default)]
    pub tree_selected_path: Option<String>,
    /// Repo-relative directory paths that were expanded in the tree.
    #[serde(default)]
    pub tree_expanded: Vec<String>,
}

/// One repository's saved view state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSession {
    pub repo: String,
    pub state: SessionState,
}

/// How many repositories' view state to remember. Beyond this the
/// least-recently-used entries are dropped, so the file cannot grow without
/// bound as repos are opened over the years.
pub const MAX_REMEMBERED: usize = 50;

/// Everything nightcrow remembers between runs: which repositories were open,
/// which tab was in front, and each repository's view state.
///
/// One file, under the config directory rather than inside any repository.
/// No single repo owns the fact that three others were open beside it, and
/// keeping view state out of the repos means nightcrow never creates a
/// directory in a project it is only reading.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceState {
    /// Absolute repo paths in tab order.
    pub repos: Vec<String>,
    /// Index into `repos` of the tab that was in front.
    #[serde(default)]
    pub active: usize,
    /// Per-repository view state, most recently used first.
    #[serde(default)]
    pub sessions: Vec<RepoSession>,
}

impl WorkspaceState {
    /// Record `state` for `repo`, moving it to the front of the
    /// least-recently-used order and evicting past `MAX_REMEMBERED`.
    pub fn remember(&mut self, repo: &str, state: SessionState) {
        self.sessions.retain(|s| s.repo != repo);
        self.sessions.insert(
            0,
            RepoSession {
                repo: repo.to_string(),
                state,
            },
        );
        self.sessions.truncate(MAX_REMEMBERED);
    }
}

/// `~/.nightcrow/workspace.json`, or `None` when the home directory cannot be
/// determined — in which case the tab list simply is not persisted.
fn workspace_path() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|h| h.join(".nightcrow").join("workspace.json"))
}

pub fn load_workspace() -> Option<WorkspaceState> {
    load_workspace_at(&workspace_path()?)
}

fn load_workspace_at(path: &Path) -> Option<WorkspaceState> {
    match crate::persistence::read_json(path) {
        Ok(state) => state,
        Err(e) => {
            tracing::warn!("corrupted workspace file, ignoring: {e}");
            None
        }
    }
}

/// Record the open tabs. Called on exit, like the per-repo sessions, so a
/// crash loses the tab list the same way it loses the rest of the session.
///
/// An empty list is written rather than skipped: closing every tab and
/// quitting is how a user asks for an empty screen next launch, and dropping
/// the write would resurrect the previous tabs instead.
pub fn save_workspace(state: &WorkspaceState) {
    let Some(path) = workspace_path() else {
        return;
    };
    save_workspace_at(&path, state);
}

fn save_workspace_at(path: &Path, state: &WorkspaceState) {
    if let Err(e) = crate::persistence::write_json(path, state) {
        tracing::warn!("failed to save workspace: {e:#}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_round_trips_the_open_tabs() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("workspace.json");
        let state = WorkspaceState {
            repos: vec!["/w/api".to_string(), "/w/web".to_string()],
            active: 1,
            sessions: Vec::new(),
        };

        save_workspace_at(&path, &state);
        let loaded = load_workspace_at(&path).expect("written file loads back");

        assert_eq!(loaded.repos, state.repos);
        assert_eq!(loaded.active, 1);
    }

    #[test]
    fn an_empty_workspace_is_recorded_rather_than_skipped() {
        // Closing every tab and quitting is how a user asks for an empty
        // screen next launch; skipping the write would restore the old tabs.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("workspace.json");
        save_workspace_at(
            &path,
            &WorkspaceState {
                repos: vec!["/w/api".to_string()],
                active: 0,
                sessions: Vec::new(),
            },
        );

        save_workspace_at(&path, &WorkspaceState::default());

        assert!(load_workspace_at(&path).unwrap().repos.is_empty());
    }

    #[test]
    fn remembering_a_repo_moves_it_to_the_front_and_evicts_the_oldest() {
        let mut state = WorkspaceState::default();
        for i in 0..MAX_REMEMBERED {
            state.remember(&format!("/w/p{i}"), SessionState::default());
        }
        // Re-recording an existing repo moves it rather than duplicating it.
        state.remember("/w/p0", SessionState::default());
        assert_eq!(state.sessions.len(), MAX_REMEMBERED);
        assert_eq!(state.sessions[0].repo, "/w/p0");

        state.remember("/w/fresh", SessionState::default());

        assert_eq!(state.sessions.len(), MAX_REMEMBERED, "capped");
        assert_eq!(state.sessions[0].repo, "/w/fresh");
        assert!(
            !state.sessions.iter().any(|s| s.repo == "/w/p1"),
            "the least recently used entry is evicted"
        );
    }

    #[test]
    fn a_corrupted_workspace_file_is_ignored() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("workspace.json");
        std::fs::write(&path, "{ not json").unwrap();

        assert!(load_workspace_at(&path).is_none());
    }

    #[test]
    fn a_missing_workspace_file_is_not_an_error() {
        let dir = tempfile::TempDir::new().unwrap();

        assert!(load_workspace_at(&dir.path().join("absent.json")).is_none());
    }
}
