use crate::app::Focus;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionState {
    pub focus: Option<Focus>,
    pub selected_file: Option<String>,
    pub scroll: usize,
    pub active_pane: usize,
}

fn session_path(repo_path: &str) -> std::path::PathBuf {
    Path::new(repo_path).join(".nightcrow").join("session.json")
}

pub fn load_session(repo_path: &str) -> SessionState {
    let path = session_path(repo_path);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return SessionState::default(),
    };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_session(repo_path: &str, state: &SessionState) {
    let path = session_path(repo_path);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = serde_json::to_string(state) {
        let _ = std::fs::write(&path, text);
    }
}
