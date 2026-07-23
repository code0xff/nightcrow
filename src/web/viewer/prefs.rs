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

/// The sidebar width the layout used before it was adjustable; a client that
/// has never dragged the divider (or an older `viewer.json` without the field)
/// gets this.
pub const DEFAULT_SIDEBAR_WIDTH: u32 = 460;
/// Bounds the stored sidebar width so a value from any client keeps both panes
/// usable: never so narrow the status letters clip, never so wide the diff is
/// squeezed out. The browser additionally caps it to a share of the viewport
/// while dragging, which is the bound that actually bites on a small screen.
pub const MIN_SIDEBAR_WIDTH: u32 = 280;
pub const MAX_SIDEBAR_WIDTH: u32 = 720;

/// Everything the viewer remembers for its clients. One shared value, not one
/// per repository: repo ids are only stable for the process lifetime
/// (`catalog.rs`), so a per-repo key would drop the preference on restart.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ViewerPrefs {
    /// Index into the accent cycle, in the TUI's order (`config::Accent::ALL`).
    pub accent: usize,
    /// File-sidebar width in CSS px, clamped to
    /// `[MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH]`. Shared across clients like the
    /// accent so every device opens at the same split.
    pub sidebar_width: u32,
}

impl Default for ViewerPrefs {
    fn default() -> Self {
        Self {
            accent: 0,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
        }
    }
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

    /// Load from an explicit path (tests, and the only injection point). A
    /// hand-edited file can carry a width outside the bounds; clamp it on load
    /// so `get` never serves a value the write path would have rejected.
    pub fn at(path: PathBuf) -> Self {
        let mut state = read(&path).unwrap_or_default();
        state.sidebar_width = state
            .sidebar_width
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        Self {
            path: Some(path),
            state: Mutex::new(state),
        }
    }

    pub fn get(&self) -> ViewerPrefs {
        *self.state.lock().expect("prefs poisoned")
    }

    /// Apply `change` and persist the result, both while holding the lock, so a
    /// second writer cannot race a stale snapshot to disk after this one: file
    /// and memory stay in step. Returns the stored value so the caller echoes
    /// back what was actually kept. A failed write is logged and the in-memory
    /// value still applies — the change the user just made must not appear to do
    /// nothing.
    fn mutate(&self, change: impl FnOnce(&mut ViewerPrefs)) -> ViewerPrefs {
        let mut state = self.state.lock().expect("prefs poisoned");
        change(&mut state);
        if let Some(path) = &self.path {
            write(path, &state);
        }
        *state
    }

    /// Apply any subset of the preferences in one locked write. A request may
    /// carry several at once (`/api/prefs` accepts both), so they must land
    /// together — otherwise a concurrent write could interleave and the echo
    /// would describe a state no single POST produced. `None` leaves a field as
    /// it is. Accent wraps into range as the TUI wraps it (`Accent::from_index`);
    /// width clamps so a browser drag past the bounds still yields a usable
    /// split.
    pub fn update(&self, accent: Option<usize>, sidebar_width: Option<u32>) -> ViewerPrefs {
        self.mutate(|state| {
            if let Some(accent) = accent {
                state.accent = accent % Accent::ALL.len();
            }
            if let Some(width) = sidebar_width {
                state.sidebar_width = width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
            }
        })
    }

    /// Store `accent` alone. Thin wrapper over [`update`] so the clamping lives
    /// in one place.
    pub fn set_accent(&self, accent: usize) -> ViewerPrefs {
        self.update(Some(accent), None)
    }

    /// Store the sidebar width alone. Thin wrapper over [`update`].
    pub fn set_sidebar_width(&self, width: u32) -> ViewerPrefs {
        self.update(None, Some(width))
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
        assert_eq!(store.get().sidebar_width, DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn a_sidebar_width_round_trips_through_the_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("viewer.json");

        PrefsStore::at(path.clone()).set_sidebar_width(500);

        assert_eq!(PrefsStore::at(path).get().sidebar_width, 500);
    }

    #[test]
    fn an_out_of_range_sidebar_width_clamps_instead_of_being_stored_as_given() {
        // The width comes from a browser drag, so it is input: a value past the
        // bounds would hand a later device a split with no diff pane, or one so
        // narrow the status letters clip.
        let dir = tempfile::TempDir::new().unwrap();
        let store = PrefsStore::at(dir.path().join("viewer.json"));

        assert_eq!(
            store
                .set_sidebar_width(MAX_SIDEBAR_WIDTH + 400)
                .sidebar_width,
            MAX_SIDEBAR_WIDTH
        );
        assert_eq!(store.set_sidebar_width(10).sidebar_width, MIN_SIDEBAR_WIDTH);
    }

    #[test]
    fn a_width_outside_the_bounds_in_the_file_is_clamped_on_load() {
        // A hand-edited file must not smuggle a value past the bounds the write
        // path enforces — `get` would otherwise serve it and an accent-only
        // write would echo it back.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("viewer.json");
        std::fs::write(&path, r#"{"accent":0,"sidebar_width":9000}"#).unwrap();

        assert_eq!(PrefsStore::at(path).get().sidebar_width, MAX_SIDEBAR_WIDTH);
    }

    #[test]
    fn an_older_file_without_a_width_keeps_its_accent_and_defaults_the_width() {
        // A `viewer.json` written before the field existed must still load: the
        // container `#[serde(default)]` fills the missing width, not zero.
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("viewer.json");
        std::fs::write(&path, r#"{"accent":3}"#).unwrap();

        let prefs = PrefsStore::at(path).get();
        assert_eq!(prefs.accent, 3);
        assert_eq!(prefs.sidebar_width, DEFAULT_SIDEBAR_WIDTH);
    }
}
