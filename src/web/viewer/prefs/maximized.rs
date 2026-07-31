//! Which panel each project was left maximized in.
//!
//! The one preference here that belongs to a *project* rather than to the
//! viewer as a whole. "How is this repository's screen arranged" is view state,
//! and the TUI has kept the same thing per repository for as long as it has had
//! a session file (`workspace/persistence.rs`: `terminal_fullscreen`,
//! `diff_fullscreen`, `list_fullscreen`). This is the browser's half of that.
//!
//! **Not written into the TUI's file, and not shared with it.** `workspace.json`
//! belongs to the TUI whenever one is attached (`ViewerState::persist`), and the
//! arrangement would not carry anyway: maximizing on a 40-row terminal and in a
//! 1400 px window are not the same answer, which is the same reason `upper_pct`
//! is kept apart from `layout.upper_pct`.
//!
//! **Keyed by absolute path, like `active_repo` and for the same reason.** Repo
//! ids only live as long as the process (`catalog.rs`), so an id on disk would
//! name nothing after a restart — which is exactly the case this exists for.
//! The server translates; the client only ever sees ids.

use serde::{Deserialize, Serialize};

/// How many projects' arrangement to remember. Past this the entries whose
/// arrangement was set longest ago go, so the file cannot grow for every
/// repository ever opened.
///
/// Ordered by when the arrangement was *set*, not when the project was last
/// looked at. Use-ordering would mean a preference write on every project
/// switch, to save a project that has had fifty others maximized after it —
/// which is the only way to reach the cap at all. The same bound the TUI puts on its own per-repo state
/// (`workspace::persistence::MAX_REMEMBERED`), for the same reason — stated
/// again rather than imported, because nothing would make the two wrong
/// together if one changed.
pub const MAX_REMEMBERED_MAXIMIZED: usize = 50;

/// The panel filling the window, when one is.
///
/// "Nothing is maximized" is the absence of an entry rather than a variant:
/// that is the overwhelmingly common state, and storing it would mean a row on
/// file for every project a person ever glanced at.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MaximizedPanel {
    Files,
    Terminal,
}

impl MaximizedPanel {
    /// Parse what a client sent. Unknown strings are `None` — the wire form is
    /// a boundary input, and a panel nobody can render is not a 500.
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
    /// Absolute worktree path. See the module header for why not the id.
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
