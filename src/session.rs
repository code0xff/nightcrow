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
    #[serde(default)]
    pub accent_idx: usize,
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

/// Which repositories were open, and which tab was in front.
///
/// Separate from `SessionState`, which is per repo and lives inside that repo.
/// This is a property of the process, so it lives with the config instead —
/// no single repo owns the fact that three others were open beside it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceState {
    /// Absolute repo paths in tab order.
    pub repos: Vec<String>,
    /// Index into `repos` of the tab that was in front.
    #[serde(default)]
    pub active: usize,
}

fn session_path(repo_path: &str) -> std::path::PathBuf {
    Path::new(repo_path).join(".nightcrow").join("session.json")
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
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(state) => Some(state),
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
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!("failed to create workspace directory: {e}");
        return;
    }
    let text = match serde_json::to_string(state) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("failed to serialize workspace: {e}");
            return;
        }
    };
    // Atomic replace, same reasoning as `save_session`.
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &text) {
        tracing::warn!("failed to write workspace tmp: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        tracing::warn!("failed to rename workspace tmp into place: {e}");
        let _ = std::fs::remove_file(&tmp_path);
    }
}

pub fn load_session(repo_path: &str) -> Option<SessionState> {
    let path = session_path(repo_path);
    let text = std::fs::read_to_string(&path).ok()?;
    match serde_json::from_str(&text) {
        Ok(state) => Some(state),
        Err(e) => {
            tracing::warn!("corrupted session file, ignoring: {e}");
            None
        }
    }
}

pub fn save_session(repo_path: &str, state: &SessionState) {
    // A repo deleted or moved while nightcrow was running must not be
    // recreated by `create_dir_all` below: the directory would come back
    // holding only `.nightcrow/`, and the next launch would restore it as a
    // tab on a path that is no longer a repository.
    if !Path::new(repo_path).is_dir() {
        tracing::warn!(repo = %repo_path, "repo is gone, not writing its session");
        return;
    }
    let path = session_path(repo_path);
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("failed to create session directory: {e}");
        }
        // Drop a self-ignoring `.gitignore` inside `.nightcrow/` so the
        // session file never pollutes the user's `git status`. Only write
        // when missing — a user-edited file should not be clobbered.
        let gi = dir.join(".gitignore");
        if !gi.exists()
            && let Err(e) = std::fs::write(&gi, "*\n")
        {
            tracing::warn!("failed to write nightcrow gitignore: {e}");
        }
    }
    let text = match serde_json::to_string(state) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("failed to serialize session: {e}");
            return;
        }
    };
    // Atomic replace: write to a sibling tmp file then rename. This keeps
    // session.json intact if the process dies mid-write.
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &text) {
        tracing::warn!("failed to write session tmp: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        tracing::warn!("failed to rename session tmp into place: {e}");
        let _ = std::fs::remove_file(&tmp_path);
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
        save_workspace_at(&path, &WorkspaceState {
            repos: vec!["/w/api".to_string()],
            active: 0,
        });

        save_workspace_at(&path, &WorkspaceState::default());

        assert!(load_workspace_at(&path).unwrap().repos.is_empty());
    }

    #[test]
    fn a_session_is_not_written_under_a_missing_repo() {
        // `create_dir_all` would otherwise resurrect the repo directory
        // holding only `.nightcrow/`, and the next launch would restore a tab
        // on a path that is no longer a repository.
        let dir = tempfile::TempDir::new().unwrap();
        let gone = dir.path().join("deleted-repo");

        save_session(&gone.to_string_lossy(), &SessionState::default());

        assert!(!gone.exists(), "the repo root must not be recreated");
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
