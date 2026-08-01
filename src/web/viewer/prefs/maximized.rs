//! Which panel each project was left maximized in.
//!
//! The one preference that belongs to a *project* rather than to the viewer as a
//! whole. Not shared with the TUI: maximizing on a 40-row terminal and in a
//! 1400 px window are not the same answer. Keyed by absolute path, like
//! `active_repo` and for the same reason — repo ids only live as long as the
//! process.

use serde::{Deserialize, Serialize};

/// How many projects' arrangement to remember. Past this the oldest entries go.
/// Ordered by when the arrangement was *set*, not when the project was last
/// looked at — use-ordering would mean a preference write on every project
/// switch. Matches the TUI's `MAX_REMEMBERED` for the same reason.
pub const MAX_REMEMBERED_MAXIMIZED: usize = 50;

/// The panel filling the window, when one is.
///
/// "Nothing is maximized" is the absence of an entry rather than a variant —
/// that is the common state, and storing it would mean a row on file for every
/// project ever glanced at.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MaximizedPanel {
    Files,
    Terminal,
}

impl MaximizedPanel {
    /// Parse what a client sent. Unknown strings are `None` — the wire form is
    /// a boundary input.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "files" => Some(Self::Files),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Terminal => "terminal",
        }
    }
}

/// One project's arrangement.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoMaximized {
    /// Absolute worktree path.
    pub repo: String,
    pub panel: MaximizedPanel,
}

/// Record `panel` for `repo`, most recently set first.
///
/// `None` un-maximizes: the entry goes rather than being stored as a "nothing"
/// state, so the list stays the set of projects that actually have an
/// arrangement to restore.
pub fn remember(list: &mut Vec<RepoMaximized>, repo: &str, panel: Option<MaximizedPanel>) {
    list.retain(|entry| entry.repo != repo);
    if let Some(panel) = panel {
        list.insert(
            0,
            RepoMaximized {
                repo: repo.to_string(),
                panel,
            },
        );
        list.truncate(MAX_REMEMBERED_MAXIMIZED);
    }
}

/// Hold a list that came off disk to what a write would have produced.
///
/// A file is not necessarily one this build wrote: it can be hand-edited, or
/// left by a version with a different cap. `remember` only ever trims the list
/// it just pushed onto, so without this an oversized one would stay oversized
/// through every later write, and a duplicated repository would keep whichever
/// entry `panel_of` happened to reach first.
pub fn normalize(list: &mut Vec<RepoMaximized>) {
    let mut seen = std::collections::HashSet::new();
    // First wins, so a duplicate resolves to the same entry `panel_of` would
    // have found — normalizing must not change what the file means.
    list.retain(|entry| seen.insert(entry.repo.clone()));
    list.truncate(MAX_REMEMBERED_MAXIMIZED);
}

/// What `repo` was left maximized in, if anything.
pub fn panel_of(list: &[RepoMaximized], repo: &str) -> Option<MaximizedPanel> {
    list.iter()
        .find(|entry| entry.repo == repo)
        .map(|entry| entry.panel)
}

#[cfg(test)]
#[path = "maximized_tests.rs"]
mod tests;
