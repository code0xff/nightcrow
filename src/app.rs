use crate::git::diff::{ChangedFile, RepoSnapshot};
mod app_impl;
mod auto_follow;
mod commit_log_apply;
mod commit_log_fetch;
mod commit_log_pagination;
mod diff_load;
mod file_view_load;
mod focus;
mod git_view_manager;
mod interaction;
mod load_apply;
mod load_controller;
mod log_nav;
mod navigation;
mod repository_view;
mod scroll;
mod session_io;
mod snapshot_io;
mod terminal_ctrl;
mod tree;
mod tree_nav;

#[cfg(test)]
pub use crate::runtime::snapshot::SnapshotChannel;
pub use crate::runtime::snapshot::SnapshotMsg;
#[cfg(test)]
pub use crate::runtime::terminal::PaneInfo;
pub use crate::runtime::terminal::TerminalState;
#[cfg(test)]
pub(crate) use crate::runtime::terminal::strip_escape_sequences;
pub use crate::ui::diff_pane::DiffPaneView;
pub use crate::ui::file_view::{FileViewKey, FileViewState};
pub use git_view_manager::GitViewManager;
pub(crate) use interaction::{InteractionState, leader_label_of};
#[cfg(test)]
pub use repository_view::RepositoryView;

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

pub struct App {
    pub(crate) git: GitViewManager,
    pub focus: Focus,
    pub notice: Option<Notice>,
    pub terminal: TerminalState,
    // Terminal focus, active pane, and fullscreen waiting on the panes.
    //
    // Panes belong to the session, so a fresh view has none until the daemon
    // reports them. A fresh launch starts with the default here and a restored
    // session replaces it with what was saved.
    pub(crate) pending_terminal: Option<crate::workspace::persistence::SessionState>,
    // Mutually exclusive with `diff.fullscreen` and `terminal.fullscreen`.
    pub list_fullscreen: bool,
    pub(crate) interaction: InteractionState,
}

#[cfg(test)]
pub(crate) mod tests;
