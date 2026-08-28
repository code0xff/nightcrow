use crate::app::{App, Focus, GitViewManager, InteractionState, Notice, NoticeKind};
use crate::backend::TerminalBackend;
use crossterm::event::KeyEvent;

impl NoticeKind {
    // `None` when the message already names its own subject (repo-input
    // rejection, session-restore note, refused project action).
    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::Git => Some("git error"),
            Self::Diff => Some("diff error"),
            Self::Terminal => Some("terminal error"),
            Self::Tree => Some("tree error"),
            Self::Session | Self::RepoInput | Self::Project => None,
        }
    }
}

impl Notice {
    pub fn new(kind: NoticeKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }

    pub fn line(&self) -> String {
        match self.kind.label() {
            Some(label) => format!("{label}: {}", self.text),
            None => self.text.clone(),
        }
    }
}

impl App {
    // The chrome has one notice row, so the newest problem wins.
    pub fn raise_notice(&mut self, kind: NoticeKind, text: impl Into<String>) {
        self.notice = Some(Notice::new(kind, text));
    }

    // Called from each subsystem's success path so a resolved problem stops
    // being reported without clobbering an unrelated one that arrived since.
    pub fn clear_notice(&mut self, kind: NoticeKind) {
        if self.notice.as_ref().is_some_and(|n| n.kind == kind) {
            self.notice = None;
        }
    }

    // Only reached for keys nightcrow itself acts on (the dispatch gate
    // excludes PTY passthrough), so typing in a terminal pane never blanks a
    // notice.
    pub fn dismiss_notice_on_app_input(&mut self) {
        self.notice = None;
    }

    // Used when the press can no longer be paired with a real release (the
    // project is leaving the screen) but the PTY is still alive — dropping the
    // record would leave that program in a drag/selection state with no
    // release ever coming.
    pub fn release_pending_press_in_place(&mut self) {
        if let Some((id, button, col, row)) = self.interaction.pending_mouse_press.take() {
            self.terminal.click_pane(id, button, false, col, row);
        }
    }

    /// Build a project view on `repo_path`, with `backend` behind its terminal
    /// panes. The backend comes from the caller: the panes belong to the
    /// session the daemon owns, and only the client connected to it can hand
    /// over the right end of that connection.
    pub fn new(
        repo_path: String,
        prompt_log: bool,
        leader: KeyEvent,
        backend: Box<dyn TerminalBackend>,
    ) -> Self {
        let app = App {
            git: GitViewManager::new(repo_path),
            focus: Focus::FileList,
            notice: None,
            terminal: crate::runtime::terminal::TerminalState::new(Some(backend), prompt_log),
            // The fresh-launch rule: the panes are not here yet, and when they
            // arrive the input focus goes to them, as it did when this view
            // opened its own PTYs on the spot. A restored session overwrites
            // this in `restore_pane_focus`.
            pending_terminal: Some(crate::workspace::persistence::SessionState {
                focus: Some(Focus::Terminal),
                ..Default::default()
            }),
            list_fullscreen: false,
            interaction: InteractionState::new(leader),
        };

        tracing::info!(repo = %app.git.repo_path(), "nightcrow started");
        app
    }

    // The repo dialog is process-level, so the full modal test lives on
    // `Workspace::overlay_active`; both feed the key and mouse handlers so a
    // click can never reach behind a modal that swallows keystrokes.
    pub fn search_overlay_active(&self) -> bool {
        self.git.view.status.search_active
            || self.git.view.tree.search_active
            || self.git.view.diff.search.active
            || self.git.view.log.commit_search_active
            || self.git.view.log.file_search_active
    }

    pub fn repository_path(&self) -> &str {
        self.git.repo_path()
    }

    pub fn repository_id(&self) -> Option<&str> {
        self.git.repo_id()
    }

    pub fn adopt_repository_id(&mut self, repo_id: String) {
        self.git.adopt_repo_id(repo_id);
    }

    pub fn mode(&self) -> crate::app::ViewMode {
        self.git.view().mode()
    }

    pub fn status_view(&self) -> &crate::ui::status_view::StatusView {
        self.git.view().status()
    }

    pub fn diff_pane(&self) -> &crate::ui::diff_pane::DiffPane {
        self.git.view().diff()
    }

    pub fn diff_pane_mut(&mut self) -> &mut crate::ui::diff_pane::DiffPane {
        self.git.view_mut().diff_mut()
    }

    pub fn log_view(&self) -> &crate::ui::log_view::LogView {
        self.git.view().log()
    }

    pub fn tree_view(&self) -> &crate::ui::tree_view::TreeView {
        self.git.view().tree()
    }

    pub fn tracking(&self) -> Option<&crate::git::diff::TrackingStatus> {
        self.git.tracking.as_ref()
    }

    pub fn branch_name(&self) -> Option<&str> {
        self.git.branch_name.as_deref()
    }

    pub fn log_decorations(&self) -> &crate::git::diff::LogDecorations {
        &self.git.log_decorations
    }

    pub fn agent_indicator_config(&self) -> &crate::config::AgentIndicatorConfig {
        &self.git.agent_indicator
    }

    pub(crate) fn configure_repository_views(
        &mut self,
        agent_indicator: crate::config::AgentIndicatorConfig,
        tree: crate::config::TreeConfig,
    ) {
        self.git.agent_indicator = agent_indicator;
        self.git.tree_config = tree;
    }

    pub(crate) fn enable_tree_watcher(&mut self) {
        self.git.view.tree_watch = crate::runtime::tree_watch::TreeWatcher::new();
    }
}
