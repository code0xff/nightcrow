//! Preferences that follow the user rather than the browser they arrived in.
//!
//! The accent lives here, not in `localStorage`, because a session is reached
//! from several places at once — phone, laptop, tablet, and the TUI itself —
//! and a per-surface copy means picking the colour again on each, then seeing
//! them disagree. It is stored in `~/.nightcrow/`, next to the workspace file,
//! so nothing is written inside a repository that is only being read.
//!
//! The accent is the session's, not the viewer's: an attached TUI reads and
//! writes this same value over the daemon socket (`web/viewer/session.rs`).
//! That crosses no security boundary — the viewer's separation from the TUI is
//! its own port, cookie, and password, and each transport still decides who may
//! ask before reaching the session at all. `sidebar_width` stays the viewer's
//! alone, having no TUI counterpart to share with.

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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ViewerPrefs {
    /// Index into the accent cycle, in the TUI's order (`config::Accent::ALL`).
    pub accent: usize,
    /// File-sidebar width in CSS px, clamped to
    /// `[MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH]`. Shared across clients like the
    /// accent so every device opens at the same split.
    pub sidebar_width: u32,
    /// Absolute worktree path of the project a client last selected, so a
    /// reload lands where the user left off instead of on the first tab.
    ///
    /// A **path**, not the repo id the client speaks: ids only live as long as
    /// the process (`catalog.rs`), so a stored id would name nothing after a
    /// restart — which is exactly the case this field exists for. The server
    /// translates in both directions; the client never learns the path.
    ///
    /// `None` until a client selects a project. Never cleared by the server: a
    /// path that stops being served just stops resolving, and the client falls
    /// back to its first tab — then records whichever project it landed on, so
    /// what is on file is always somewhere a client actually was.
    pub active_repo: Option<String>,
}

impl Default for ViewerPrefs {
    fn default() -> Self {
        Self {
            accent: 0,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            active_repo: None,
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
        Self::load_seeded(ViewerPrefs::default().accent)
    }

    /// Load from `~/.nightcrow/viewer.json`, starting at `seed_accent` when
    /// there is no file to read.
    ///
    /// The seed is `[theme] name` — the colour a session that has never been
    /// given one comes up in. Only a missing or unreadable file takes it: once
    /// the file exists it records a choice somebody made, and a later config
    /// edit must not reach back and repaint the session behind them.
    pub fn load_seeded(seed_accent: usize) -> Self {
        let seeded = seeded_prefs(seed_accent);
        match default_path() {
            Some(path) => Self::at_or(path, seeded),
            None => Self {
                path: None,
                state: Mutex::new(seeded),
            },
        }
    }

    /// Load from an explicit path (tests, and the only injection point).
    pub fn at(path: PathBuf) -> Self {
        Self::at_or(path, ViewerPrefs::default())
    }

    /// Load from `path`, falling back to `absent` when there is nothing to read.
    /// A hand-edited file can carry a width outside the bounds; clamp it on load
    /// so `get` never serves a value the write path would have rejected.
    fn at_or(path: PathBuf, absent: ViewerPrefs) -> Self {
        let mut state = read(&path).unwrap_or(absent);
        state.sidebar_width = state
            .sidebar_width
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        Self {
            path: Some(path),
            state: Mutex::new(state),
        }
    }

    pub fn get(&self) -> ViewerPrefs {
        self.state.lock().expect("prefs poisoned").clone()
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
        state.clone()
    }

    /// Apply any subset of the preferences in one locked write. A request may
    /// carry several at once (`/api/prefs` accepts any subset), so they must
    /// land together — otherwise a concurrent write could interleave and the
    /// echo would describe a state no single POST produced. `None` leaves a
    /// field as it is. Accent wraps into range as the TUI wraps it
    /// (`Accent::from_index`); width clamps so a browser drag past the bounds
    /// still yields a usable split. `active_repo` is taken as given — the
    /// caller resolved it from a live repository, so there is no range to fold
    /// it into.
    pub fn update(&self, change: PrefsUpdate) -> ViewerPrefs {
        self.mutate(|state| {
            if let Some(accent) = change.accent {
                state.accent = accent % Accent::ALL.len();
            }
            if let Some(width) = change.sidebar_width {
                state.sidebar_width = width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
            }
            if let Some(path) = change.active_repo {
                state.active_repo = Some(path);
            }
        })
    }

    /// Store `accent` alone. Thin wrapper over [`update`] so the clamping lives
    /// in one place.
    pub fn set_accent(&self, accent: usize) -> ViewerPrefs {
        self.update(PrefsUpdate {
            accent: Some(accent),
            ..PrefsUpdate::default()
        })
    }

    /// Store the sidebar width alone. Thin wrapper over [`update`].
    pub fn set_sidebar_width(&self, width: u32) -> ViewerPrefs {
        self.update(PrefsUpdate {
            sidebar_width: Some(width),
            ..PrefsUpdate::default()
        })
    }

    /// Store the active project's absolute path alone. Thin wrapper over
    /// [`update`]. There is deliberately no way to clear it: closing the last
    /// project leaves no path worth recording, and keeping the old one means it
    /// is still the selection when that project is opened again.
    pub fn set_active_repo(&self, path: String) -> ViewerPrefs {
        self.update(PrefsUpdate {
            active_repo: Some(path),
            ..PrefsUpdate::default()
        })
    }
}

/// The preferences a single write may carry, each `None` when the request left
/// it alone. A struct rather than positional arguments because several fields
/// share a type — two adjacent `Option<u32>` at a call site would swap silently.
#[derive(Debug, Clone, Default)]
pub struct PrefsUpdate {
    pub accent: Option<usize>,
    pub sidebar_width: Option<u32>,
    pub active_repo: Option<String>,
}

/// Defaults with the accent a config seed asks for. Wrapped into the cycle here
/// rather than trusted: `[theme]` is a hand-edited file, and an index with no
/// colour behind it would reach every reader of the prefs.
fn seeded_prefs(accent: usize) -> ViewerPrefs {
    ViewerPrefs {
        accent: accent % Accent::ALL.len(),
        ..ViewerPrefs::default()
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
mod tests;
