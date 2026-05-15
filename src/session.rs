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
}

fn session_path(repo_path: &str) -> std::path::PathBuf {
    Path::new(repo_path).join(".nightcrow").join("session.json")
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
