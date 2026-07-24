use crate::app::{App, AutoFollow, Notice, NoticeKind, ViewMode, Focus};
use crate::backend::{PtyBackend, TerminalBackend};
use crate::runtime::snapshot::SnapshotChannel;
use crossterm::event::KeyEvent;

impl NoticeKind {
    /// Prefix shown before the message, or `None` when the message already
    /// reads on its own (a repo-input rejection, a session-restore note, or a
    /// refused project action names its own subject).
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

    /// The single line to render, label included when the kind carries one.
    pub fn line(&self) -> String {
        match self.kind.label() {
            Some(label) => format!("{label}: {}", self.text),
            None => self.text.clone(),
        }
    }
}

impl App {
    /// Raise a notice, replacing whatever was showing. The chrome has one
    /// notice row, so the newest problem wins.
    pub fn raise_notice(&mut self, kind: NoticeKind, text: impl Into<String>) {
        self.notice = Some(Notice::new(kind, text));
    }

    /// Drop the current notice if it was raised by `kind`. Called from the
    /// success path of each subsystem, so a resolved problem stops being
    /// reported without clobbering an unrelated one that arrived since.
    pub fn clear_notice(&mut self, kind: NoticeKind) {
        if self.notice.as_ref().is_some_and(|n| n.kind == kind) {
            self.notice = None;
        }
    }

    /// Drop the current notice because the user acted on the app itself.
    ///
    /// Deliberately *not* called for keys forwarded to a PTY: in a terminal
    /// pane every keystroke is passthrough, so dismissing on those would make
    /// a notice vanish the instant the user resumed typing — the same
    /// effectively-invisible failure this row exists to prevent.
    /// Deliver a pending press's release to the pane that saw it, at the cell
    /// the press landed on.
    ///
    /// Used when the press can no longer be paired with a real release — the
    /// project is leaving the screen — but the PTY is still alive. Dropping
    /// the record instead would leave that program in a drag or selection
    /// state with no release ever coming.
    pub fn release_pending_press_in_place(&mut self) {
        if let Some((id, button, col, row)) = self.pending_mouse_press.take() {
            self.terminal.click_pane(id, button, false, col, row);
        }
    }

    pub fn dismiss_notice_on_app_input(&mut self) {
        self.notice = None;
    }

    pub fn new(
        repo_path: String,
        prompt_log: bool,
        startup_commands: &[crate::config::StartupCommand],
        leader: KeyEvent,
    ) -> Self {
        let snapshot = SnapshotChannel::spawn(&repo_path);

        let backend: Box<dyn TerminalBackend> = Box::new(PtyBackend::new(&repo_path));

        let mut app = App {
            mode: ViewMode::Status,
            status_view: crate::ui::status_view::StatusView::default(),
            diff: crate::ui::diff_pane::DiffPane::default(),
            focus: Focus::FileList,
            notice: None,
            repo_path,
            log_view: crate::ui::log_view::LogView::default(),
            tree_view: crate::ui::tree_view::TreeView::default(),
            terminal: crate::runtime::terminal::TerminalState::new(Some(backend), prompt_log),
            accent_idx: 0,
            tracking: None,
            snapshot,
            pending_snapshot: None,
            // Start disabled; `main` upgrades to a live watcher after the parsed
            // `[tree] live_watch` config is applied, so a `false` setting never
            // spawns an OS watcher.
            tree_watch: crate::runtime::tree_watch::TreeWatcher::disabled(),
            tree_dirty: Default::default(),
            tree_dirty_all: false,
            pending_selection: None,
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
            leader,
            prefix_armed: false,
            awaiting_swap_target: false,
            pending_mouse_press: None,
            mouse_enabled: true,
        };

        app.ensure_initial_terminal(startup_commands);
        tracing::info!(repo = %app.repo_path, "nightcrow started");
        app
    }

    /// True while the leader has been pressed and we await the follow-up key.
    /// Drives the hint bar's `PREFIX` indicator.
    pub fn prefix_armed(&self) -> bool {
        self.prefix_armed
    }

    /// Arm the prefix: the next key will be interpreted as an app command.
    pub fn arm_prefix(&mut self) {
        self.prefix_armed = true;
    }

    /// Disarm the prefix, returning to normal pass-through routing.
    pub fn cancel_prefix(&mut self) {
        self.prefix_armed = false;
    }

    /// Whether one of this project's search bars owns input right now. The
    /// repo dialog is process-level, so the full modal test lives on
    /// `Workspace::overlay_active`; both feed the key and mouse handlers so a
    /// click can never reach behind a modal that swallows keystrokes.
    pub fn search_overlay_active(&self) -> bool {
        self.status_view.search_active
            || self.tree_view.search_active
            || self.diff.search.active
            || self.log_view.commit_search_active
            || self.log_view.file_search_active
    }

    /// True while `<leader> s` armed pane-swap mode and we await the target
    /// digit. Drives the hint bar's `SWAP` indicator.
    pub fn awaiting_swap_target(&self) -> bool {
        self.awaiting_swap_target
    }

    /// Arm pane-swap mode: the next digit picks the pane to swap with the
    /// active pane. Clears the prefix so the two follow-up states never overlap.
    pub fn begin_swap_target(&mut self) {
        self.prefix_armed = false;
        self.awaiting_swap_target = true;
    }

    /// Disarm pane-swap mode without acting.
    pub fn cancel_swap_target(&mut self) {
        self.awaiting_swap_target = false;
    }

    /// Caret-notation label for the configured leader chord, e.g. `^F` for
    /// `Ctrl+F`. Leaders are always ctrl chords (see `config::parse_leader`),
    /// so the control character maps cleanly to `^<UPPER>`; any non-ctrl key
    /// falls back to printing its raw character.
    pub fn leader_label(&self) -> String {
        crate::app::leader_label_of(self.leader)
    }

    /// True when `key` matches the configured leader chord. Any modifier beyond
    /// the leader's own (Alt, Shift, Super, Hyper, Meta — enhanced keyboard
    /// protocols report the latter three) makes it a different chord that passes
    /// straight through to the PTY instead of being swallowed, so we compare the
    /// full modifier set exactly.
    pub fn is_leader_key(&self, key: KeyEvent) -> bool {
        key.code == self.leader.code && key.modifiers == self.leader.modifiers
    }
}