use crate::config::{AgentIndicatorConfig, LogConfig, TreeConfig};
use crate::git::diff::{LogDecorations, TrackingStatus};
use crate::runtime::snapshot::{SnapshotChannel, SnapshotMsg};
#[cfg(test)]
use crate::runtime::tree_watch::TreeWatcher;

use super::commit_log_pagination::CommitLogController;
use super::load_controller::LoadController;
use super::repository_view::RepositoryView;

pub struct GitViewManager {
    pub(crate) repo_path: String,
    pub(crate) repo_id: Option<String>,
    pub(crate) view: RepositoryView,
    pub(crate) repo_cache: Option<git2::Repository>,
    pub(crate) snapshot: SnapshotChannel,
    pub(crate) pending_snapshot: Option<SnapshotMsg>,
    pub(crate) commit_log: CommitLogController,
    pub(crate) branch_name: Option<String>,
    pub(crate) tracking: Option<TrackingStatus>,
    pub(crate) log_decorations: LogDecorations,
    pub(crate) last_refs_fingerprint: Option<u64>,
    pub(crate) load_controller: LoadController,
    pub(crate) agent_indicator: AgentIndicatorConfig,
    pub(crate) tree_config: TreeConfig,
}

impl GitViewManager {
    pub fn new(repo_path: String) -> Self {
        let snapshot = SnapshotChannel::spawn(&repo_path);
        Self::from_parts(repo_path, snapshot, RepositoryView::default())
    }

    fn from_parts(repo_path: String, snapshot: SnapshotChannel, view: RepositoryView) -> Self {
        let log = LogConfig::default();
        Self {
            repo_path,
            repo_id: None,
            view,
            repo_cache: None,
            snapshot,
            pending_snapshot: None,
            commit_log: CommitLogController::with_config(
                log.commit_log_page_size,
                log.commit_log_prefetch_threshold,
            ),
            branch_name: None,
            tracking: None,
            log_decorations: LogDecorations::default(),
            last_refs_fingerprint: None,
            load_controller: LoadController::new(),
            agent_indicator: AgentIndicatorConfig::default(),
            tree_config: TreeConfig::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn from_test_parts(
        repo_path: String,
        snapshot: SnapshotChannel,
        tree_watch: TreeWatcher,
    ) -> Self {
        Self::from_parts(
            repo_path,
            snapshot,
            RepositoryView::default().with_tree_watcher(tree_watch),
        )
    }

    pub fn repo_path(&self) -> &str {
        &self.repo_path
    }

    pub fn repo_id(&self) -> Option<&str> {
        self.repo_id.as_deref()
    }

    pub fn adopt_repo_id(&mut self, repo_id: String) {
        self.repo_id = Some(repo_id);
    }

    pub fn view(&self) -> &RepositoryView {
        &self.view
    }

    pub fn view_mut(&mut self) -> &mut RepositoryView {
        &mut self.view
    }

    #[cfg(test)]
    pub(crate) fn pending_snapshot(&self) -> Option<&SnapshotMsg> {
        self.pending_snapshot.as_ref()
    }
}
