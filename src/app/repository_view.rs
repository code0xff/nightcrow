use std::collections::BTreeSet;
use std::time::{Instant, SystemTime};

use crate::runtime::tree_watch::TreeWatcher;
use crate::ui::diff_pane::DiffPane;
use crate::ui::log_view::LogView;
use crate::ui::status_view::StatusView;
use crate::ui::tree_view::TreeView;

use super::ViewMode;

#[derive(Default)]
pub struct AutoFollow {
    pub last_manual_nav_at: Option<Instant>,
    pub followed_path: Option<String>,
}

pub struct RepositoryView {
    pub(crate) mode: ViewMode,
    pub(crate) status: StatusView,
    pub(crate) log: LogView,
    pub(crate) tree: TreeView,
    pub(crate) diff: DiffPane,
    pub(crate) auto_follow: AutoFollow,
    pub(crate) selected_snapshot_mtime: Option<(String, Option<SystemTime>)>,
    pub(crate) tree_watch: TreeWatcher,
    pub(crate) tree_dirty: BTreeSet<String>,
    pub(crate) tree_dirty_all: bool,
    pub(crate) pending_selection: Option<(String, usize)>,
}

impl Default for RepositoryView {
    fn default() -> Self {
        Self {
            mode: ViewMode::Status,
            status: StatusView::default(),
            log: LogView::default(),
            tree: TreeView::default(),
            diff: DiffPane::default(),
            auto_follow: AutoFollow::default(),
            selected_snapshot_mtime: None,
            tree_watch: TreeWatcher::disabled(),
            tree_dirty: BTreeSet::new(),
            tree_dirty_all: false,
            pending_selection: None,
        }
    }
}

impl RepositoryView {
    pub fn mode(&self) -> ViewMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: ViewMode) {
        self.mode = mode;
    }

    pub fn status(&self) -> &StatusView {
        &self.status
    }

    pub fn status_mut(&mut self) -> &mut StatusView {
        &mut self.status
    }

    pub fn log(&self) -> &LogView {
        &self.log
    }

    pub fn tree(&self) -> &TreeView {
        &self.tree
    }

    pub fn diff(&self) -> &DiffPane {
        &self.diff
    }

    pub fn diff_mut(&mut self) -> &mut DiffPane {
        &mut self.diff
    }

    pub fn set_pending_selection(&mut self, selection: Option<(String, usize)>) {
        self.pending_selection = selection;
    }

    pub fn pending_selection(&self) -> Option<&(String, usize)> {
        self.pending_selection.as_ref()
    }

    pub fn take_pending_selection(&mut self) -> Option<(String, usize)> {
        self.pending_selection.take()
    }

    #[cfg(test)]
    pub(crate) fn with_tree_watcher(mut self, tree_watch: TreeWatcher) -> Self {
        self.tree_watch = tree_watch;
        self
    }
}
