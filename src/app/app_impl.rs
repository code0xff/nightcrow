use crate::app::{App, AutoFollow, Focus, InteractionState, Notice, NoticeKind, ViewMode};
use crate::backend::TerminalBackend;
use crate::runtime::snapshot::SnapshotChannel;
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

    // NOT called for keys forwarded to a PTY: in a terminal pane every
    // keystroke is passthrough, so dismissing on those would make a notice
    // vanish the instant the user resumed typing.
    //
    // Used when the press can no longer be paired with a real release (the
    // project is leaving the screen) but the PTY is still alive — dropping the
    // record would leave that program in a drag/selection state with no
    // release ever coming.
    pub fn release_pending_press_in_place(&mut self) {
        if let Some((id, button, col, row)) = self.interaction.pending_mouse_press.take() {
            self.terminal.click_pane(id, button, false, col, row);
        }
    }

    pub fn dismiss_notice_on_app_input(&mut self) {
        self.notice = None;
    }

    /// Build a project view on `repo_path`, with `backend` behind its terminal
    /// panes.
    ///
    /// The backend comes from the caller because where the panes live is not
    /// this type's decision: they belong to the session the daemon owns, and
    /// only the client that connected to it can hand over the right end of that
    /// connection.
    pub fn new(
        repo_path: String,
        prompt_log: bool,
        leader: KeyEvent,
        backend: Box<dyn TerminalBackend>,
    ) -> Self {
        let snapshot = SnapshotChannel::spawn(&repo_path);

        let app = App {
            mode: ViewMode::Status,
            status_view: crate::ui::status_view::StatusView::default(),
            diff: crate::ui::diff_pane::DiffPane::default(),
            focus: Focus::FileList,
            notice: None,
            repo_path,
            repo_id: None,
            log_view: crate::ui::log_view::LogView::default(),
            tree_view: crate::ui::tree_view::TreeView::default(),
            terminal: crate::runtime::terminal::TerminalState::new(Some(backend), prompt_log),
            tracking: None,
            snapshot,
            pending_snapshot: None,
            // `main` upgrades to a live watcher after applying `[tree] live_watch`,
            // so a `false` setting never spawns an OS watcher.
            tree_watch: crate::runtime::tree_watch::TreeWatcher::disabled(),
            tree_dirty: Default::default(),
            tree_dirty_all: false,
            pending_selection: None,
            // The fresh-launch rule: the panes are not here yet, and when they
            // arrive the input focus goes to them, as it did when this view
            // opened its own PTYs on the spot. A restored session overwrites
            // this in `restore_pane_focus`.
            pending_terminal: Some(crate::workspace::persistence::SessionState {
                focus: Some(Focus::Terminal),
                ..Default::default()
            }),
            repo_cache: None,
            cfg_agent_indicator: crate::config::AgentIndicatorConfig::default(),
            cfg_tree: crate::config::TreeConfig::default(),
            pagination: crate::app::commit_log_pagination::CommitLogPagination::with_config(
                crate::config::LogConfig::default().commit_log_page_size,
                crate::config::LogConfig::default().commit_log_prefetch_threshold,
            ),
            auto_follow: AutoFollow::default(),
            list_fullscreen: false,
            branch_name: None,
            log_decorations: Default::default(),
            last_refs_fingerprint: None,
            interaction: InteractionState::new(leader),
        };

        tracing::info!(repo = %app.repo_path, "nightcrow started");
        app
    }

    // The repo dialog is process-level, so the full modal test lives on
    // `Workspace::overlay_active`; both feed the key and mouse handlers so a
    // click can never reach behind a modal that swallows keystrokes.
    pub fn search_overlay_active(&self) -> bool {
        self.status_view.search_active
            || self.tree_view.search_active
            || self.diff.search.active
            || self.log_view.commit_search_active
            || self.log_view.file_search_active
    }
}
