use crate::git::diff::{ChangedFile, RepoSnapshot, TrackingStatus};
mod app_impl;
mod auto_follow;
mod commit_log_apply;
mod commit_log_fetch;
mod commit_log_pagination;
mod diff_load;
mod file_view_load;
mod focus;
mod log_nav;
mod navigation;
mod scroll;
mod session_io;
mod snapshot_io;
mod terminal_ctrl;
mod tree;
mod tree_nav;

pub use crate::app::commit_log_pagination::CommitLogPagination;
pub use crate::runtime::snapshot::{SnapshotChannel, SnapshotMsg};
#[cfg(test)]
pub use crate::runtime::terminal::PaneInfo;
pub use crate::runtime::terminal::TerminalState;
#[cfg(test)]
pub(crate) use crate::runtime::terminal::strip_escape_sequences;
pub use crate::ui::diff_pane::{DiffPane, DiffPaneView};
pub use crate::ui::file_view::{FileViewKey, FileViewState};
pub use crate::ui::log_view::LogView;
pub use crate::ui::status_view::StatusView;
pub use crate::ui::tree_view::TreeView;
use crossterm::event::{KeyEvent, KeyModifiers};
use std::time::Instant;

pub(crate) const LIST_PAGE_SIZE: usize = 10;
pub(crate) const DIFF_PAGE_SIZE: usize = 20;

// Keying expiry on the variant (not message text) forces a decision about
// when each new kind goes away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Git,
    Diff,
    Terminal,
    Tree,
    Session,
    RepoInput,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

// Free function so the empty screen (no project) can label its hints too.
pub fn leader_label_of(leader: KeyEvent) -> String {
    match leader.code {
        crossterm::event::KeyCode::Char(c) if leader.modifiers.contains(KeyModifiers::CONTROL) => {
            format!("^{}", c.to_ascii_uppercase())
        }
        crossterm::event::KeyCode::Char(c) => c.to_string(),
        _ => "<prefix>".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum ViewMode {
    #[default]
    Status,
    Log,
    Tree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Focus {
    FileList,
    DiffViewer,
    Terminal,
}

// Auto-follow state: idle timer + last-steered path.
#[derive(Default)]
pub struct AutoFollow {
    pub last_manual_nav_at: Option<Instant>,
    pub followed_path: Option<String>,
}

pub struct App {
    pub mode: ViewMode,
    pub status_view: StatusView,
    pub diff: DiffPane,
    pub focus: Focus,
    pub notice: Option<Notice>,
    pub repo_path: String,
    /// The daemon's opaque id for this repository, once attached.
    ///
    /// `None` when running without a daemon, and until the first set arrives.
    pub repo_id: Option<String>,
    pub log_view: LogView,
    pub tree_view: TreeView,
    pub terminal: TerminalState,
    pub tracking: Option<TrackingStatus>,
    pub(crate) snapshot: SnapshotChannel,
    // Set by `drain_snapshot` (every project), consumed by `poll_snapshot`
    // (active only) — a background project's git work defers until its tab shows.
    pub(crate) pending_snapshot: Option<SnapshotMsg>,
    // Filesystem watcher for live tree refresh; active only in `ViewMode::Tree`.
    pub(crate) tree_watch: crate::runtime::tree_watch::TreeWatcher,
    // Watcher-touched directories not yet re-read. Filled by `drain_tree_watcher`
    // (every project), consumed by `poll_tree_watcher` (active only).
    pub(crate) tree_dirty: std::collections::BTreeSet<String>,
    // Set when events were dropped/unattributed: next refresh re-reads everything.
    pub(crate) tree_dirty_all: bool,
    // Saved selection waiting on the first snapshot.
    pub(crate) pending_selection: Option<(String, usize)>,
    // Terminal focus, active pane, and fullscreen waiting on the panes.
    //
    // Panes belong to the session, so a fresh view has none until the daemon
    // reports them. A fresh launch starts with the default here and a restored
    // session replaces it with what was saved.
    pub(crate) pending_terminal: Option<crate::workspace::persistence::SessionState>,
    // Cached `git2::Repository` for sync loads. Opened lazily, invalidated in
    // `change_repo`. The snapshot worker keeps its own handle (`!Send`).
    pub(crate) repo_cache: Option<git2::Repository>,
    pub cfg_agent_indicator: crate::config::AgentIndicatorConfig,
    pub cfg_tree: crate::config::TreeConfig,
    // Drop impl joins the worker so `change_repo` can't leak the old-repo fetch.
    pub pagination: CommitLogPagination,
    pub auto_follow: AutoFollow,
    // Mutually exclusive with `diff.fullscreen` and `terminal.fullscreen`.
    pub list_fullscreen: bool,
    // `None` for detached HEAD / unborn branch / bare repo.
    pub branch_name: Option<String>,
    // Ref chips and ahead/behind sets for the Log view. Rebuilt only when
    // `last_refs_fingerprint` disagrees with the newest snapshot's.
    pub log_decorations: crate::git::diff::LogDecorations,
    pub(crate) last_refs_fingerprint: Option<u64>,
    pub leader: KeyEvent,
    // No timeout: stays armed until a follow-up key or `Esc`/`Ctrl+C` resolves it.
    pub prefix_armed: bool,
    // Mutually exclusive with `prefix_armed` (arming this clears the prefix).
    pub awaiting_swap_target: bool,
    // A release pairs with the press's pane, not the pane under the pointer.
    // Single slot — a second press overwrites (no multi-button).
    pub pending_mouse_press: Option<(
        crate::backend::PaneId,
        crossterm::event::MouseButton,
        u16,
        u16,
    )>,
    // Mirror of `[mouse] enabled`. Gates only the hint bar's clickability.
    pub mouse_enabled: bool,
}

#[cfg(test)]
pub(crate) mod tests;
