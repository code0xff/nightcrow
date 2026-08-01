//! Preferences that follow the user rather than the browser they arrived in.
//!
//! Stored in `~/.nightcrow/viewer.json`. The accent is the session's (shared
//! with an attached TUI); `sidebar_width` and `upper_pct` are the viewer's alone
//! — the first has no TUI counterpart, and the second is deliberately not shared
//! because a percentage means different things on a terminal vs a browser window.

pub mod maximized;
pub use maximized::{MaximizedPanel, RepoMaximized};

use crate::config::Accent;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Default sidebar width before the divider was dragged.
pub const DEFAULT_SIDEBAR_WIDTH: u32 = 460;
/// Bounds the stored sidebar width so both panes stay usable.
pub const MIN_SIDEBAR_WIDTH: u32 = 280;
pub const MAX_SIDEBAR_WIDTH: u32 = 720;

/// Default split share for the diff panel (matches the TUI's `layout.upper_pct`).
pub const DEFAULT_UPPER_PCT: u32 = 55;
/// Bounds the split so neither half becomes a sliver.
pub const MIN_UPPER_PCT: u32 = 20;
pub const MAX_UPPER_PCT: u32 = 85;

/// Everything the viewer remembers for its clients.
///
/// Preferences are shared across clients, with `maximized` the single exception.
/// Repo ids are only stable for the process lifetime, so per-repo keys are stored
/// by **path** instead — the same reason `active_repo` is a path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ViewerPrefs {
    /// Index into the accent cycle (`config::Accent::ALL`).
    pub accent: usize,
    /// File-sidebar width in CSS px, clamped to `[MIN, MAX]`.
    pub sidebar_width: u32,
    /// Share of the vertical split given to the diff panel, in percent.
    ///
    /// The viewer's own, not the session's — unlike the accent. A percentage
    /// means different things on a terminal vs a browser window, so sharing with
    /// the TUI's `layout.upper_pct` was rejected.
    pub upper_pct: u32,
    /// Absolute worktree path of the last-selected project.
    ///
    /// A **path**, not the repo id: ids only live as long as the process, so a
    /// stored id would name nothing after a restart. The server translates; the
    /// client never learns the path. `None` until a client selects a project.
    pub active_repo: Option<String>,
    /// Which panel each project was left maximized in, most recently set first.
    pub maximized: Vec<RepoMaximized>,
}

impl Default for ViewerPrefs {
    fn default() -> Self {
        Self {
            accent: 0,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            upper_pct: DEFAULT_UPPER_PCT,
            active_repo: None,
            maximized: Vec::new(),
        }
    }
}

/// The stored preferences plus the file they are written to. A missing or
/// corrupt file yields defaults.
pub struct PrefsStore {
    /// `None` when the home directory cannot be determined (preferences apply
    /// for the process lifetime but are not persisted).
    path: Option<PathBuf>,
    state: Mutex<ViewerPrefs>,
}

impl PrefsStore {
    /// Load from `~/.nightcrow/viewer.json`, starting at `seed_accent` when
    /// there is no file to read. The seed is `[theme] name` — only a missing or
    /// unreadable file takes it; once the file exists it records a choice.
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

    /// Load from an explicit path (tests).
    #[cfg(test)]
    pub fn at(path: PathBuf) -> Self {
        Self::at_or(path, ViewerPrefs::default())
    }

    /// Load from `path`, falling back to `absent` when there is nothing to read.
    /// Clamps widths and splits on load so `get` never serves a value the write
    /// path would have rejected.
    fn at_or(path: PathBuf, absent: ViewerPrefs) -> Self {
        let mut state = read(&path).unwrap_or(absent);
        state.sidebar_width = state
            .sidebar_width
            .clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
        state.upper_pct = state.upper_pct.clamp(MIN_UPPER_PCT, MAX_UPPER_PCT);
        maximized::normalize(&mut state.maximized);
        Self {
            path: Some(path),
            state: Mutex::new(state),
        }
    }

    pub fn get(&self) -> ViewerPrefs {
        self.state.lock().expect("prefs poisoned").clone()
    }

    /// Apply `change` and persist the result under the lock, so file and memory
    /// stay in step. A failed write is logged and the in-memory value still
    /// applies.
    fn mutate(&self, change: impl FnOnce(&mut ViewerPrefs)) -> ViewerPrefs {
        let mut state = self.state.lock().expect("prefs poisoned");
        change(&mut state);
        if let Some(path) = &self.path {
            write(path, &state);
        }
        state.clone()
    }

    /// Apply any subset of the preferences in one locked write. `None` leaves a
    /// field as it is. Accent wraps into range; width clamps.
    pub fn update(&self, change: PrefsUpdate) -> ViewerPrefs {
        self.mutate(|state| {
            if let Some(accent) = change.accent {
                state.accent = accent % Accent::ALL.len();
            }
            if let Some(width) = change.sidebar_width {
                state.sidebar_width = width.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH);
            }
            if let Some(pct) = change.upper_pct {
                state.upper_pct = pct.clamp(MIN_UPPER_PCT, MAX_UPPER_PCT);
            }
            if let Some(path) = change.active_repo {
                state.active_repo = Some(path);
            }
            if let Some(change) = change.maximized {
                maximized::remember(&mut state.maximized, &change.repo, change.panel);
            }
        })
    }

    /// Record how one project's screen is arranged. `None` un-maximizes.
    #[cfg(test)]
    pub fn set_maximized(&self, repo: String, panel: Option<MaximizedPanel>) -> ViewerPrefs {
        self.update(PrefsUpdate {
            maximized: Some(MaximizedUpdate { repo, panel }),
            ..PrefsUpdate::default()
        })
    }

    /// Store `accent` alone.
    pub fn set_accent(&self, accent: usize) -> ViewerPrefs {
        self.update(PrefsUpdate {
            accent: Some(accent),
            ..PrefsUpdate::default()
        })
    }

    /// Store the sidebar width alone.
    #[cfg(test)]
    pub fn set_sidebar_width(&self, width: u32) -> ViewerPrefs {
        self.update(PrefsUpdate {
            sidebar_width: Some(width),
            ..PrefsUpdate::default()
        })
    }

    /// Store the split percentage alone.
    #[cfg(test)]
    pub fn set_upper_pct(&self, pct: u32) -> ViewerPrefs {
        self.update(PrefsUpdate {
            upper_pct: Some(pct),
            ..PrefsUpdate::default()
        })
    }

    /// Store the active project's absolute path alone. There is deliberately no
    /// way to clear it: closing the last project leaves no path worth recording,
    /// and keeping the old one means it is still the selection when that project
    /// is opened again.
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
    pub upper_pct: Option<u32>,
    pub active_repo: Option<String>,
    pub maximized: Option<MaximizedUpdate>,
}

/// One project's arrangement, as a write carries it. Two `Option`s deep because
/// the outer one means "this request said nothing about maximizing" and the
/// inner one means "this project is no longer maximized" — collapsing them
/// would make un-maximizing indistinguishable from not mentioning it.
#[derive(Debug, Clone)]
pub struct MaximizedUpdate {
    /// Absolute worktree path, resolved by the caller from a live repository.
    pub repo: String,
    pub panel: Option<MaximizedPanel>,
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
    match crate::persistence::read_json(path) {
        Ok(prefs) => prefs,
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
    if let Err(e) = crate::persistence::write_json(path, prefs) {
        tracing::warn!("failed to save viewer prefs: {e:#}");
    }
}

#[cfg(test)]
mod tests;
