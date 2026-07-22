//! Viewer preferences that follow the user rather than the browser.
//!
//! The accent lives here, not in `localStorage`, because the viewer is reached
//! from several devices — phone, laptop, tablet — and a per-browser copy means
//! picking the colour again on each one. It is stored in `~/.nightcrow/`, next
//! to the workspace file, so the viewer never writes inside a repository it is
//! only reading.
//!
//! Deliberately *not* the TUI's setting: `[theme]` and the TUI's per-repo
//! `accent_idx` (`session.rs`) stay untouched, keeping the viewer's separation
//! from the TUI (own port, own cookie, own password) intact. This is the
//! viewer's own preference, shared across the viewer's own clients.

use crate::config::Accent;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Everything the viewer remembers for its clients. One shared value, not one
/// per repository: repo ids are only stable for the process lifetime
/// (`catalog.rs`), so a per-repo key would drop the preference on restart.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ViewerPrefs {
    /// Index into the accent cycle, in the TUI's order (`config::Accent::ALL`).
    pub accent: usize,
}

/// The stored preferences plus the file they are written to. A missing,
/// unreadable, or corrupt file yields defaults — a colour preference is never
/// worth failing a request over.
pub struct PrefsStore {
    /// `None` when the home directory cannot be determined, in which case
    /// preferences apply for the process lifetime but are not persisted.
    path: Option<PathBuf>,
    state: Mutex<ViewerPrefs>,
}

impl PrefsStore {
    /// Load from `~/.nightcrow/viewer.json`.
    pub fn load() -> Self {
        match default_path() {
            Some(path) => Self::at(path),
            None => Self {
                path: None,
                state: Mutex::new(ViewerPrefs::default()),
            },
        }
    }

    /// Load from an explicit path (tests, and the only injection point).
    pub fn at(path: PathBuf) -> Self {
        let state = read(&path).unwrap_or_default();
        Self {
            path: Some(path),
            state: Mutex::new(state),
        }
    }

    pub fn get(&self) -> ViewerPrefs {
        *self.state.lock().expect("prefs poisoned")
    }

    /// Store `accent`, wrapped into range the same way the TUI wraps it
    /// (`Accent::from_index`), and persist. Returns the stored value so the
    /// caller can echo back what was actually kept rather than what was asked
    /// for. A failed write is logged and the in-memory value still applies:
    /// the click the user just made must not appear to do nothing.
    pub fn set_accent(&self, accent: usize) -> ViewerPrefs {
        let updated = {
            let mut state = self.state.lock().expect("prefs poisoned");
            state.accent = accent % Accent::ALL.len();
            *state
        };
        if let Some(path) = &self.path {
            write(path, &updated);
        }
        updated
    }
}

fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".nightcrow").join("viewer.json"))
}

fn read(path: &Path) -> Option<ViewerPrefs> {
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(prefs) => Some(prefs),
        Err(e) => {
            tracing::warn!("corrupted viewer prefs file, ignoring: {e}");
            None
        }
    }
}

/// Write via a temporary file and rename, so a crash mid-write leaves the
/// previous preferences rather than a truncated file — the same handling
/// `session.rs` gives the workspace.
fn write(path: &Path, prefs: &ViewerPrefs) {
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        tracing::warn!("failed to create viewer prefs directory: {e}");
        return;
    }
    let text = match serde_json::to_string(prefs) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("failed to serialize viewer prefs: {e}");
            return;
        }
    };
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &text) {
        tracing::warn!("failed to write viewer prefs tmp: {e}");
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, path) {
        tracing::warn!("failed to rename viewer prefs tmp into place: {e}");
        let _ = std::fs::remove_file(&tmp_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_accent_round_trips_through_the_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("viewer.json");

        PrefsStore::at(path.clone()).set_accent(3);

        assert_eq!(PrefsStore::at(path).get().accent, 3);
    }

    #[test]
    fn an_out_of_range_accent_wraps_instead_of_being_stored_as_given() {
        // The index comes from a browser, so it is input: storing it verbatim
        // would hand every later reader a value with no colour behind it.
        let dir = tempfile::TempDir::new().unwrap();
        let store = PrefsStore::at(dir.path().join("viewer.json"));

        let stored = store.set_accent(Accent::ALL.len() + 2);

        assert_eq!(stored.accent, 2);
        assert_eq!(store.get().accent, 2);
    }

    #[test]
    fn a_corrupt_file_reads_as_defaults_rather_than_failing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("viewer.json");
        std::fs::write(&path, "{not json").unwrap();

        assert_eq!(PrefsStore::at(path).get(), ViewerPrefs::default());
    }

    #[test]
    fn a_missing_file_reads_as_defaults() {
        let dir = tempfile::TempDir::new().unwrap();

        let store = PrefsStore::at(dir.path().join("absent.json"));

        assert_eq!(store.get().accent, 0);
    }
}
