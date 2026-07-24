use crate::git::diff::{ChangedFile, RepoSnapshot, TrackingStatus};
mod auto_follow;
mod app_impl;
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

/// What raised a notice — the key its expiry is scoped to.
///
/// Expiry used to be decided by matching the message text
/// (`msg.starts_with("git error:")`), which tied clearing to human-readable
/// prose and only ever covered the two kinds that happened to have a matching
/// arm: terminal, tree, and session messages were never cleared at all and sat
/// in the chrome until the repo was switched. Keying on the variant instead
/// means adding a kind forces a decision about when it goes away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Git,
    Diff,
    Terminal,
    Tree,
    Session,
    RepoInput,
    /// A refused workspace-level action (tab cap reached, last tab closed).
    Project,
}

/// A message shown in the chrome's notice row until its kind expires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: String,
}

/// Caret-notation label for a leader chord. Free function so the empty
/// screen, which has no project to ask, can label its hints too.
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
    /// Read-only directory-tree navigator rooted at the workdir.
    Tree,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Focus {
    FileList,
    DiffViewer,
    Terminal,
}

/// State that drives the auto-follow behaviour: keep track of when the user
/// last navigated manually (so an active user is never hijacked) and the path
/// auto-follow last steered selection to (so it doesn't repeatedly assert the
/// same hot file). The behaviour config (`cfg_agent_indicator`) stays on
/// `App` because the file-list renderer also reads it.
#[derive(Default)]
pub struct AutoFollow {
    /// Wall-clock instant of the most recent user-driven selection change in
    /// the file list. `None` means "idle since boot".
    pub last_manual_nav_at: Option<Instant>,
    /// Path the auto-follow last steered selection to. Prevents repeatedly
    /// re-asserting selection on the same already-hot-and-selected file.
    pub followed_path: Option<String>,
}

pub struct App {
    pub mode: ViewMode,
    pub status_view: StatusView,
    pub diff: DiffPane,
    pub focus: Focus,
    pub notice: Option<Notice>,
    pub repo_path: String,
    pub log_view: LogView,
    pub tree_view: TreeView,
    pub terminal: TerminalState,
    pub accent_idx: usize,
    pub tracking: Option<TrackingStatus>,
    pub(crate) snapshot: SnapshotChannel,
    /// Latest snapshot drained from the worker but not yet applied. Set by
    /// `drain_snapshot` (which every project runs) and consumed by
    /// `poll_snapshot` (which only the active project runs), so a background
    /// project's git work is deferred until its tab is shown.
    pub(crate) pending_snapshot: Option<SnapshotMsg>,
    /// Filesystem watcher driving live refresh of the file-tree navigator. Only
    /// active while in `ViewMode::Tree`; watches the expanded directories
    /// (non-recursively) and triggers a cache re-read on change. Inert when the
    /// OS watcher could not start, in which case refresh-on-entry is the
    /// fallback.
    pub(crate) tree_watch: crate::runtime::tree_watch::TreeWatcher,
    /// Directories a watcher event touched but which have not been re-read
    /// yet. Filled by `drain_tree_watcher` (which every project runs) and
    /// consumed by `poll_tree_watcher` (only the active one), so a hidden
    /// project's tree refreshes when its tab is shown rather than rereading
    /// directories on the UI thread meanwhile.
    pub(crate) tree_dirty: std::collections::BTreeSet<String>,
    /// Set when events were dropped or could not be attributed, so the next
    /// refresh must re-read everything instead of trusting `tree_dirty`.
    pub(crate) tree_dirty_all: bool,
    /// A saved file selection waiting on the first snapshot, with its diff
    /// scroll. The only part of a session that cannot be applied on the spot:
    /// it names a file the changed-file list has not delivered yet. Nothing
    /// the user does can conflict with it, since an empty list offers nothing
    /// to select.
    pub(crate) pending_selection: Option<(String, usize)>,
    /// Cached `git2::Repository` for synchronous loads (file diff, commit
    /// diff, file blob, commit log). Opened lazily on first use; invalidated
    /// in `change_repo`. The snapshot worker thread keeps its own handle —
    /// `git2::Repository` is `!Send` and cannot be shared.
    pub(crate) repo_cache: Option<git2::Repository>,
    pub cfg_agent_indicator: crate::config::AgentIndicatorConfig,
    /// Behaviour config for the file-tree navigator (`.gitignore` filtering,
    /// max expansion depth). Read by the tree navigation/preview methods.
    pub cfg_tree: crate::config::TreeConfig,
    /// Commit-log page sizing, prefetch threshold, in-flight worker, and
    /// the HEAD anchor used to detect external commits. See
    /// `CommitLogPagination`. The Drop impl on the struct joins the
    /// worker so a `change_repo` cannot leak the old-repo fetch.
    pub pagination: CommitLogPagination,
    /// Auto-follow state (idle timer + last-steered path). Behaviour config
    /// lives separately on `cfg_agent_indicator` since the file-list
    /// renderer also reads it.
    pub auto_follow: AutoFollow,
    /// True while the upper-left list panel (file list in Status mode, commit
    /// list in Log mode) is rendered full-screen. Mutually exclusive with
    /// `diff.fullscreen` and `terminal.fullscreen`.
    pub list_fullscreen: bool,
    /// Current branch shorthand carried in the latest snapshot. `None` for
    /// detached HEAD, unborn branches, or bare repos. Rendered in the top
    /// header so the user always sees which branch the workdir tracks.
    pub branch_name: Option<String>,
    /// The configured leader (prefix) chord. Pressing it arms `prefix_armed`;
    /// the next key is then interpreted as an app command (tmux-style).
    pub leader: KeyEvent,
    /// True while the leader has been pressed and we are waiting for the
    /// follow-up key. There is intentionally NO timeout: the prefix stays
    /// armed until a follow-up key (mapped → run + disarm, unmapped → consume
    /// + disarm) or `Esc`/`Ctrl+C` (cancel) resolves it.
    pub prefix_armed: bool,
    /// True while `<leader> s` has armed pane-swap mode and we await the digit
    /// that names the swap target. Mutually exclusive with `prefix_armed`:
    /// arming this clears the prefix, so both are never set at once. Resolved by
    /// the next key (digit → swap + disarm, `Esc`/`Ctrl+C` → cancel, anything
    /// else → consume + disarm), with no timeout — same model as the prefix.
    pub awaiting_swap_target: bool,
    /// The pane and button of a forwarded mouse press whose release has not
    /// been seen yet. A release pairs with the press's pane, not the pane
    /// under the pointer: drag reports are not forwarded, so the program
    /// that saw the press must see the release even when the pointer moved
    /// off the pane in between. Single slot — a second press before the
    /// first release overwrites it (multi-button chords are not paired).
    /// Pane, button, and pane-local cell of a forwarded press whose release
    /// has not been seen. The cell is kept so the press can still be released
    /// where it happened when no pointer position is available — switching
    /// projects, for instance.
    pub pending_mouse_press: Option<(
        crate::backend::PaneId,
        crossterm::event::MouseButton,
        u16,
        u16,
    )>,
    /// Mirror of `[mouse] enabled`. Gates only the hint bar's clickability
    /// inversion — with capture off no mouse event ever arrives, so the
    /// input path needs no check, but a label must not advertise a click
    /// that cannot happen.
    pub mouse_enabled: bool,
}

#[cfg(test)]
pub(crate) mod tests;
