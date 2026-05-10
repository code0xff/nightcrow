use crate::backend::BackendEvent;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use crate::git::diff::{
    ChangedFile, CommitEntry, DiffHunk, RepoSnapshot, TrackingStatus, load_commit_diff_with_repo,
    load_commit_file_blob_with_repo, load_commit_file_diff_with_repo, load_commit_files_with_repo,
    load_commit_log_with_repo, load_file_diff_with_repo, load_snapshot,
    load_workdir_file_with_repo, parse_hunk_new_start,
};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

const SCROLLBACK_LINES: usize = 1000;
const LIST_PAGE_SIZE: usize = 10;
const DIFF_PAGE_SIZE: usize = 20;
const COMMIT_LOG_LIMIT: usize = 500;

/// Move a list index up by `n`, saturating at 0. Returns `true` when the index
/// actually changed so callers can decide whether to refresh associated state.
fn cursor_up(idx: &mut usize, n: usize) -> bool {
    let next = idx.saturating_sub(n);
    if next != *idx {
        *idx = next;
        true
    } else {
        false
    }
}

/// Move a list index down by `n`, clamped to `len - 1`. Returns `true` when the
/// index actually changed. A zero-length list is a no-op.
fn cursor_down(idx: &mut usize, len: usize, n: usize) -> bool {
    if len == 0 {
        return false;
    }
    let next = idx.saturating_add(n).min(len - 1);
    if next != *idx {
        *idx = next;
        true
    } else {
        false
    }
}

/// Owns the receiver and stop channel for the background snapshot thread.
/// Dropping the struct (and its `_stop_tx`) signals the thread to exit.
pub struct SnapshotChannel {
    rx: Receiver<SnapshotMsg>,
    // Held only for its drop side-effect: dropping the sender unblocks
    // the worker's recv_timeout so it can observe the disconnect.
    _stop_tx: SyncSender<()>,
}

impl SnapshotChannel {
    pub fn spawn(repo_path: &str) -> Self {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(0);
        let path = repo_path.to_string();
        thread::spawn(move || {
            loop {
                let msg = match load_snapshot(&path) {
                    Ok(s) => SnapshotMsg::Ok(s),
                    Err(e) => SnapshotMsg::Err(e.to_string()),
                };
                if tx.send(msg).is_err() {
                    break;
                }
                match stop_rx.recv_timeout(Duration::from_millis(1000)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        Self {
            rx,
            _stop_tx: stop_tx,
        }
    }

    pub fn try_recv(&self) -> Result<SnapshotMsg, mpsc::TryRecvError> {
        self.rx.try_recv()
    }
}

fn strip_escape_sequences(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.peek().copied() {
                Some('[') => {
                    // CSI: consume parameter/intermediate bytes (0x20–0x3f), stop at
                    // final byte (0x40–0x7e). Break early on control chars to avoid
                    // consuming content that follows a malformed sequence.
                    chars.next();
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                        if c < '\x20' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC: skip until BEL or ST
                    chars.next();
                    loop {
                        match chars.next() {
                            None | Some('\x07') => break,
                            Some('\x1b') if chars.peek() == Some(&'\\') => {
                                chars.next();
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            },
            '\r' | '\n' => result.push(ch),
            c if !c.is_control() => result.push(c),
            _ => {}
        }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ViewMode {
    #[default]
    Status,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Focus {
    FileList,
    DiffViewer,
    Terminal,
}

pub enum SnapshotMsg {
    Ok(RepoSnapshot),
    Err(String),
}

pub struct PaneInfo {
    pub id: PaneId,
    pub title: String,
}

pub struct TerminalState {
    pub panes: Vec<PaneInfo>,
    pub active: usize,
    pub size: (u16, u16),
    pub scroll: HashMap<PaneId, usize>,
    pub fullscreen: bool,
    parsers: HashMap<PaneId, vt100::Parser>,
    prompt_bufs: HashMap<PaneId, String>,
    prompt_log_enabled: bool,
    backend: Option<Box<dyn TerminalBackend>>,
}

impl TerminalState {
    pub fn new(backend: Option<Box<dyn TerminalBackend>>, prompt_log_enabled: bool) -> Self {
        Self {
            panes: Vec::new(),
            active: 0,
            size: (22, 78),
            scroll: HashMap::new(),
            fullscreen: false,
            parsers: HashMap::new(),
            prompt_bufs: HashMap::new(),
            prompt_log_enabled,
            backend,
        }
    }
}

#[derive(Default)]
pub struct StatusView {
    pub files: Vec<ChangedFile>,
    pub selected: usize,
    pub file_scroll_x: usize,
    pub search_query: String,
    search_query_lower: String,
    pub search_active: bool,
    /// Indices into `files` that match the current `search_query`.
    /// Recomputed only when `files` or the query changes (see
    /// `App::recompute_status_filter`). Read-only for renderers.
    filter_cache: Vec<usize>,
}

impl StatusView {
    /// Refresh `filter_cache` from `files` and the current query. Callers must
    /// invoke this after mutating `files`, `search_query`, or
    /// `search_query_lower`; otherwise the cache will diverge from state.
    fn recompute_filter(&mut self) {
        self.filter_cache.clear();
        if self.search_query.is_empty() {
            self.filter_cache.extend(0..self.files.len());
            return;
        }
        let q = self.search_query_lower.as_str();
        for (i, f) in self.files.iter().enumerate() {
            if f.path.to_lowercase().contains(q) {
                self.filter_cache.push(i);
            }
        }
    }
}

#[derive(Default)]
pub struct RepoInput {
    pub active: bool,
    pub buf: String,
}

/// Post-load behaviour for `apply_diff_result`. Replaces the prior 3-flag
/// signature where the combination of `reset_scroll` and `keep_scroll` was
/// hard to parse at call sites.
enum DiffApply<'a> {
    /// Reset scroll/cursor to top after a successful load.
    Reset,
    /// Keep the previous scroll position (for in-place refresh).
    KeepScroll(usize),
    /// Reset scroll and additionally update the log diff title.
    ResetWithTitle(&'a str),
}

#[derive(Default)]
pub struct LogView {
    pub commits: Vec<CommitEntry>,
    pub selected: usize,
    pub diff_title: String,
    pub drill_down: bool,
    pub commit_files: Vec<ChangedFile>,
    pub file_selected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffPaneView {
    #[default]
    Diff,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileViewKey {
    Status(String),
    Commit { oid: git2::Oid, path: String },
}

#[derive(Default)]
pub struct FileViewState {
    pub key: Option<FileViewKey>,
    pub content: String,
    pub scroll: usize,
    pub anchor_line: Option<usize>,
    pub error: Option<String>,
}

impl FileViewState {
    pub fn line_count(&self) -> usize {
        if self.content.is_empty() {
            return 0;
        }
        // count() on lines() drops a trailing empty line; that's fine for scroll bounds.
        self.content.lines().count()
    }
}

#[derive(Default)]
pub struct DiffSearch {
    pub active: bool,
    pub query: String,
    query_lower: String,
    pub matches: Vec<usize>,
    pub cursor: usize,
}

impl DiffSearch {
    pub fn is_visible(&self) -> bool {
        self.active || !self.query.is_empty()
    }

    pub fn has_query(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn current_match(&self) -> Option<usize> {
        self.matches.get(self.cursor).copied()
    }

    pub fn is_match(&self, flat_idx: usize) -> bool {
        self.matches.binary_search(&flat_idx).is_ok()
    }

    fn start(&mut self) {
        self.active = true;
    }

    fn confirm(&mut self) {
        self.active = false;
    }

    fn clear(&mut self) {
        self.active = false;
        self.query.clear();
        self.query_lower.clear();
        self.matches.clear();
        self.cursor = 0;
    }

    fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.query_lower = self.query.to_lowercase();
    }

    fn pop_char(&mut self) {
        self.query.pop();
        self.query_lower = self.query.to_lowercase();
    }

    fn next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        self.cursor = (self.cursor + 1) % self.matches.len();
        self.current_match()
    }

    fn prev(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        if self.cursor == 0 {
            self.cursor = self.matches.len() - 1;
        } else {
            self.cursor -= 1;
        }
        self.current_match()
    }
}

pub struct App {
    pub mode: ViewMode,
    pub status_view: StatusView,
    pub hunks: Vec<DiffHunk>,
    pub scroll: usize,
    pub diff_scroll_x: usize,
    pub focus: Focus,
    pub status: Option<String>,
    pub repo_path: String,
    pub log_view: LogView,
    pub terminal: TerminalState,
    pub repo_input: RepoInput,
    pub diff_search: DiffSearch,
    pub diff_pane_view: DiffPaneView,
    pub file_view: FileViewState,
    pub accent_idx: usize,
    pub tracking: Option<TrackingStatus>,
    snapshot: SnapshotChannel,
    pending_session: Option<crate::session::SessionState>,
    /// Cached `git2::Repository` for synchronous loads (file diff, commit
    /// diff, file blob, commit log). Opened lazily on first use; invalidated
    /// in `change_repo`. The snapshot worker thread keeps its own handle —
    /// `git2::Repository` is `!Send` and cannot be shared.
    repo_cache: Option<git2::Repository>,
}

impl App {
    pub fn new(repo_path: String, prompt_log: bool) -> Self {
        let snapshot = SnapshotChannel::spawn(&repo_path);

        let backend: Box<dyn TerminalBackend> = Box::new(PtyBackend::new(&repo_path));

        let mut app = App {
            mode: ViewMode::Status,
            status_view: StatusView::default(),
            hunks: Vec::new(),
            scroll: 0,
            diff_scroll_x: 0,
            focus: Focus::FileList,
            status: None,
            repo_path,
            log_view: LogView::default(),
            terminal: TerminalState::new(Some(backend), prompt_log),
            repo_input: RepoInput::default(),
            diff_search: DiffSearch::default(),
            diff_pane_view: DiffPaneView::default(),
            file_view: FileViewState::default(),
            accent_idx: 0,
            tracking: None,
            snapshot,
            pending_session: None,
            repo_cache: None,
        };

        app.ensure_initial_terminal();
        tracing::info!(repo = %app.repo_path, "nightcrow started");
        app
    }

    fn ensure_initial_terminal(&mut self) {
        if self.terminal.backend.is_none() {
            return;
        }

        if let Err(err) = self.create_terminal_pane() {
            self.status = Some(format!("terminal error: {err}"));
        }
    }

    pub fn poll_snapshot(&mut self) {
        while let Ok(msg) = self.snapshot.try_recv() {
            match msg {
                SnapshotMsg::Ok(snapshot) => {
                    let previous_path = self
                        .status_view
                        .files
                        .get(self.status_view.selected)
                        .map(|f| f.path.clone());
                    self.status_view.files = snapshot.files;
                    self.status_view.recompute_filter();
                    self.tracking = snapshot.tracking;

                    let selected_path_changed =
                        self.restore_selection(previous_path.as_deref()) != previous_path;
                    self.refresh_diff(selected_path_changed);
                    if self
                        .status
                        .as_deref()
                        .is_some_and(|msg| msg.starts_with("git error:"))
                    {
                        self.status = None;
                    }
                    if let Some(state) = self.pending_session.take() {
                        self.restore_session(&state);
                    }
                }
                SnapshotMsg::Err(e) => {
                    tracing::warn!(error = %e, "git snapshot failed");
                    self.status = Some(format!("git error: {e}"));
                }
            }
        }
    }

    pub fn poll_terminal(&mut self) {
        let events: Vec<BackendEvent> = self
            .terminal
            .backend
            .as_mut()
            .map(|b| b.drain_events())
            .unwrap_or_default();

        for event in events {
            match event {
                BackendEvent::Output { pane, data } => {
                    if let Some(parser) = self.terminal.parsers.get_mut(&pane) {
                        parser.process(&data);
                    }
                }
                BackendEvent::Exited { pane } => {
                    // Single source of truth for pane removal: drain_events no
                    // longer touches the backend's pane map, so we drive the
                    // teardown here. destroy_pane is idempotent against a
                    // pane that close_active_pane already removed.
                    if let Some(backend) = &mut self.terminal.backend {
                        backend.destroy_pane(pane);
                    }
                    self.remove_terminal_pane_state(pane);
                    self.terminal.panes.retain(|p| p.id != pane);
                    self.clamp_active_pane_after_removal();
                }
            }
        }
    }

    pub fn open_new_pane(&mut self) {
        if let Err(e) = self.create_terminal_pane() {
            tracing::error!("create_terminal_pane failed: {e}");
            self.status = Some(format!("terminal error: {e}"));
        }
    }

    pub fn create_terminal_pane(&mut self) -> anyhow::Result<()> {
        let (rows, cols) = self.terminal.size;
        let backend = self
            .terminal
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        let id = backend.create_pane(rows.max(1), cols.max(1))?;
        let parser = vt100::Parser::new(rows.max(1), cols.max(1), SCROLLBACK_LINES);
        self.terminal.parsers.insert(id, parser);
        self.terminal.panes.push(PaneInfo {
            id,
            title: "shell".to_string(),
        });
        self.terminal.active = self.terminal.panes.len() - 1;
        tracing::info!(pane = id, "terminal pane opened");
        Ok(())
    }

    pub fn close_active_pane(&mut self) {
        let Some(info) = self.terminal.panes.get(self.terminal.active) else {
            return;
        };
        let id = info.id;
        tracing::info!(pane = id, "terminal pane closed");
        if let Some(backend) = &mut self.terminal.backend {
            backend.destroy_pane(id);
        }
        self.remove_terminal_pane_state(id);
        self.terminal.panes.remove(self.terminal.active);
        self.clamp_active_pane_after_removal();
    }

    fn remove_terminal_pane_state(&mut self, id: PaneId) {
        self.terminal.parsers.remove(&id);
        // Flush any unterminated prompt input so we don't lose the line the
        // user was composing when the pane closes.
        if let Some(buf) = self.terminal.prompt_bufs.remove(&id)
            && !buf.is_empty()
        {
            tracing::info!(target: "prompt", pane = id, text = %buf);
        }
        self.terminal.scroll.remove(&id);
    }

    fn clamp_active_pane_after_removal(&mut self) {
        if self.terminal.panes.is_empty() {
            self.terminal.active = 0;
            self.focus = Focus::DiffViewer;
            self.terminal.fullscreen = false;
        } else {
            self.terminal.active = self.terminal.active.min(self.terminal.panes.len() - 1);
        }
    }

    pub fn change_repo(&mut self, new_path: String) {
        // Replacing _stop_tx drops the old sender, signaling the old thread to exit.
        // Replacing the channel drops the old _stop_tx, signaling the old
        // worker to exit at its next recv_timeout boundary.
        self.snapshot = SnapshotChannel::spawn(&new_path);
        if let Some(ref mut backend) = self.terminal.backend {
            // Only future panes adopt the new cwd; existing shells stay in
            // their original directory so we don't disrupt commands already
            // running in them. Users who want the new cwd everywhere can
            // close existing panes (ctrl+w) and open fresh ones (ctrl+t).
            backend.set_cwd(std::path::Path::new(&new_path));
        }
        tracing::info!(path = %new_path, "repo changed");
        self.repo_path = new_path;
        // Drop the cached Repository — it points to the previous repo's .git
        // directory and would silently keep returning stale results.
        self.repo_cache = None;
        self.mode = ViewMode::Status;
        self.status_view.files.clear();
        self.status_view.selected = 0;
        self.hunks.clear();
        self.scroll = 0;
        self.diff_scroll_x = 0;
        self.status_view.file_scroll_x = 0;
        self.log_view.commits.clear();
        self.log_view.selected = 0;
        self.log_view.diff_title.clear();
        self.reset_drill_down_state();
        self.status_view.search_query.clear();
        self.status_view.search_query_lower.clear();
        self.status_view.search_active = false;
        self.status_view.recompute_filter();
        self.diff_search.clear();
        self.status = None;
        self.tracking = None;
        self.focus = Focus::FileList;
    }

    pub fn start_repo_input(&mut self) {
        self.repo_input.buf = self.repo_path.clone();
        self.repo_input.active = true;
    }

    pub fn cancel_repo_input(&mut self) {
        self.repo_input.active = false;
        self.repo_input.buf.clear();
    }

    pub fn confirm_repo_input(&mut self) {
        self.repo_input.active = false;
        let path = std::mem::take(&mut self.repo_input.buf);
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.status = Some("repo path cannot be empty".to_string());
            return;
        }
        let p = std::path::Path::new(trimmed);
        if !p.is_dir() {
            self.status = Some(format!("not a directory: {trimmed}"));
            return;
        }
        self.change_repo(trimmed.to_string());
    }

    pub fn repo_input_push(&mut self, ch: char) {
        self.repo_input.buf.push(ch);
    }

    pub fn repo_input_pop(&mut self) {
        self.repo_input.buf.pop();
    }

    pub fn switch_pane(&mut self, idx: usize) {
        if idx < self.terminal.panes.len() {
            self.terminal.active = idx;
            self.focus = Focus::Terminal;
        }
    }

    pub fn send_terminal_input(&mut self, data: &[u8]) {
        if let Some(info) = self.terminal.panes.get(self.terminal.active) {
            let id = info.id;
            self.terminal.scroll.remove(&id);
            if let Some(backend) = &mut self.terminal.backend
                && let Err(e) = backend.send_input(id, data)
            {
                tracing::warn!("failed to send terminal input to pane {id}: {e}");
            }
            if self.terminal.prompt_log_enabled {
                self.buffer_prompt_input(id, data);
            }
        }
    }

    pub fn scroll_terminal_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(id) = self.active_pane_id() {
            let offset = self.terminal.scroll.entry(id).or_insert(0);
            *offset = offset.saturating_add(lines);
        }
    }

    pub fn scroll_terminal_down(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(id) = self.active_pane_id()
            && let Some(entry) = self.terminal.scroll.get_mut(&id)
        {
            *entry = entry.saturating_sub(lines);
            if *entry == 0 {
                self.terminal.scroll.remove(&id);
            }
        }
    }

    pub fn is_terminal_scrolled(&self) -> bool {
        self.active_pane_id()
            .and_then(|id| self.terminal.scroll.get(&id))
            .is_some_and(|&v| v > 0)
    }

    pub fn sync_terminal_scroll(&mut self) {
        let Some(id) = self.active_pane_id() else {
            return;
        };
        let offset = self.terminal.scroll.get(&id).copied().unwrap_or(0);
        let actual = match self.terminal.parsers.get_mut(&id) {
            Some(parser) => {
                // vt100 visible_rows() computes `rows_len - scrollback_offset` without
                // saturating_sub, panicking when offset exceeds the screen height.
                let screen_rows = parser.screen().size().0 as usize;
                parser.set_scrollback(offset.min(screen_rows));
                parser.screen().scrollback()
            }
            None => return,
        };
        if actual == 0 {
            self.terminal.scroll.remove(&id);
        } else {
            self.terminal.scroll.insert(id, actual);
        }
    }

    fn buffer_prompt_input(&mut self, pane_id: PaneId, data: &[u8]) {
        let text = strip_escape_sequences(data);
        let buf = self.terminal.prompt_bufs.entry(pane_id).or_default();
        for ch in text.chars() {
            if ch == '\r' || ch == '\n' {
                if !buf.is_empty() {
                    tracing::info!(target: "prompt", pane = pane_id, text = %buf);
                    buf.clear();
                }
            } else {
                buf.push(ch);
            }
        }
    }

    pub fn resize_terminal_panes(&mut self, rows: u16, cols: u16) {
        if self.terminal.size == (rows, cols) {
            return;
        }
        self.terminal.size = (rows, cols);
        let r = rows.max(1);
        let c = cols.max(1);
        for info in &self.terminal.panes {
            if let Some(backend) = &mut self.terminal.backend {
                backend.resize(info.id, r, c);
            }
            if let Some(parser) = self.terminal.parsers.get_mut(&info.id) {
                parser.set_size(r, c);
            }
        }
    }

    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.terminal.panes.get(self.terminal.active).map(|p| p.id)
    }

    pub fn active_screen(&self) -> Option<&vt100::Screen> {
        let id = self.active_pane_id()?;
        self.terminal.parsers.get(&id).map(|p| p.screen())
    }

    pub fn reload_diff(&mut self) {
        self.refresh_diff(true);
    }

    /// Run `f` with the cached `git2::Repository`, opening it lazily on first
    /// use. Cache is invalidated by `change_repo` so that follow-up calls open
    /// a fresh handle for the new path. Errors from the open propagate so the
    /// caller can surface them in `self.status`.
    fn with_repo<R>(
        &mut self,
        f: impl FnOnce(&git2::Repository) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        if self.repo_cache.is_none() {
            let repo = git2::Repository::discover(self.repo_path.as_str())
                .map_err(|e| anyhow::anyhow!("not a git repository: {e}"))?;
            self.repo_cache = Some(repo);
        }
        // unwrap is sound: we just inserted Some above when None.
        f(self.repo_cache.as_ref().unwrap())
    }

    fn refresh_diff(&mut self, reset_scroll: bool) {
        if self.mode == ViewMode::Log {
            return;
        }
        let previous_scroll = self.scroll;
        let path = self
            .status_view
            .files
            .get(self.status_view.selected)
            .map(|f| f.path.clone());
        let Some(path) = path else {
            self.clear_diff_state();
            return;
        };
        let result = self.with_repo(|repo| load_file_diff_with_repo(repo, &path));
        if let Err(e) = &result {
            tracing::debug!(error = %e, file = %path, "failed to load diff");
        }
        let mode = if reset_scroll {
            DiffApply::Reset
        } else {
            DiffApply::KeepScroll(previous_scroll)
        };
        self.apply_diff_result(result, mode);
    }

    /// Centralizes the post-load shape used by every diff loader: on success
    /// stash hunks, reset/restore scroll and search cursor, optionally update
    /// the log title, and recompute diff search matches; on error clear state
    /// but preserve the title so the user knows what failed.
    fn apply_diff_result(&mut self, result: anyhow::Result<Vec<DiffHunk>>, mode: DiffApply<'_>) {
        let reset_scroll = matches!(mode, DiffApply::Reset | DiffApply::ResetWithTitle(_));
        match result {
            Ok(hunks) => {
                self.hunks = hunks;
                match mode {
                    DiffApply::Reset | DiffApply::ResetWithTitle(_) => {
                        self.scroll = 0;
                        self.diff_scroll_x = 0;
                        self.diff_search.cursor = 0;
                        self.invalidate_file_view();
                    }
                    DiffApply::KeepScroll(prev) => {
                        self.scroll = prev;
                    }
                }
                if let DiffApply::ResetWithTitle(title) = mode {
                    self.log_view.diff_title = title.to_string();
                }
                if !self.diff_search.query.is_empty() {
                    self.recompute_diff_matches(reset_scroll);
                }
            }
            Err(_) => {
                self.clear_diff_state();
                if let DiffApply::ResetWithTitle(title) = mode {
                    self.log_view.diff_title = title.to_string();
                }
            }
        }
    }

    fn clear_diff_state(&mut self) {
        self.hunks.clear();
        self.diff_search.matches.clear();
        self.diff_search.cursor = 0;
        self.scroll = 0;
        self.diff_scroll_x = 0;
        self.invalidate_file_view();
    }

    fn invalidate_file_view(&mut self) {
        self.diff_pane_view = DiffPaneView::Diff;
        self.file_view = FileViewState::default();
    }

    fn current_file_view_key(&self) -> Option<FileViewKey> {
        match self.mode {
            ViewMode::Status => {
                let path = self
                    .status_view
                    .files
                    .get(self.status_view.selected)?
                    .path
                    .clone();
                Some(FileViewKey::Status(path))
            }
            ViewMode::Log => {
                if !self.log_view.drill_down {
                    return None;
                }
                let oid = self.log_view.commits.get(self.log_view.selected)?.oid;
                let path = self
                    .log_view
                    .commit_files
                    .get(self.log_view.file_selected)?
                    .path
                    .clone();
                Some(FileViewKey::Commit { oid, path })
            }
        }
    }

    fn load_file_view(&mut self, key: FileViewKey) {
        let result = match key.clone() {
            FileViewKey::Status(path) => {
                self.with_repo(|repo| load_workdir_file_with_repo(repo, &path))
            }
            FileViewKey::Commit { oid, path } => {
                self.with_repo(|repo| load_commit_file_blob_with_repo(repo, oid, &path))
            }
        };
        let anchor = self
            .hunks
            .iter()
            .find_map(|h| parse_hunk_new_start(&h.header));
        let mut fv = FileViewState {
            key: Some(key),
            anchor_line: anchor,
            ..Default::default()
        };
        match result {
            Ok(content) => {
                fv.scroll = anchor
                    .map(|n| n.saturating_sub(1).saturating_sub(2))
                    .unwrap_or(0);
                fv.content = content;
            }
            Err(e) => {
                fv.error = Some(e.to_string());
            }
        }
        self.file_view = fv;
    }

    pub fn toggle_diff_file_view(&mut self) {
        if self.diff_pane_view == DiffPaneView::File {
            self.diff_pane_view = DiffPaneView::Diff;
            return;
        }
        let Some(key) = self.current_file_view_key() else {
            return;
        };
        if self.file_view.key.as_ref() != Some(&key) {
            self.load_file_view(key);
        }
        self.diff_pane_view = DiffPaneView::File;
    }

    pub fn file_view_max_scroll(&self) -> usize {
        self.file_view.line_count().saturating_sub(1)
    }

    pub fn file_view_scroll_up(&mut self, n: usize) {
        self.file_view.scroll = self.file_view.scroll.saturating_sub(n);
    }

    pub fn file_view_scroll_down(&mut self, n: usize) {
        self.file_view.scroll = self
            .file_view
            .scroll
            .saturating_add(n)
            .min(self.file_view_max_scroll());
    }

    fn restore_selection(&mut self, previous_path: Option<&str>) -> Option<String> {
        if self.status_view.files.is_empty() {
            self.status_view.selected = 0;
            return None;
        }

        if let Some(path) = previous_path
            && let Some(index) = self
                .status_view
                .files
                .iter()
                .position(|file| file.path == path)
        {
            self.status_view.selected = index;
            return Some(path.to_string());
        }

        self.status_view.selected = self
            .status_view
            .selected
            .min(self.status_view.files.len().saturating_sub(1));
        self.status_view
            .files
            .get(self.status_view.selected)
            .map(|file| file.path.clone())
    }

    pub fn filtered_indices(&self) -> &[usize] {
        &self.status_view.filter_cache
    }

    pub fn start_search(&mut self) {
        self.status_view.search_active = true;
    }

    pub fn cancel_search(&mut self) {
        self.status_view.search_active = false;
        self.status_view.search_query.clear();
        self.status_view.search_query_lower.clear();
        self.status_view.recompute_filter();
    }

    pub fn confirm_search(&mut self) {
        self.status_view.search_active = false;
    }

    pub fn search_push(&mut self, ch: char) {
        self.status_view.search_query.push(ch);
        self.status_view.search_query_lower = self.status_view.search_query.to_lowercase();
        self.status_view.recompute_filter();
        self.clamp_to_filtered();
    }

    pub fn search_pop(&mut self) {
        self.status_view.search_query.pop();
        self.status_view.search_query_lower = self.status_view.search_query.to_lowercase();
        self.status_view.recompute_filter();
        self.clamp_to_filtered();
    }

    pub fn start_diff_search(&mut self) {
        self.diff_search.start();
    }

    pub fn cancel_diff_search(&mut self) {
        self.diff_search.clear();
    }

    pub fn confirm_diff_search(&mut self) {
        self.diff_search.confirm();
    }

    pub fn diff_search_push(&mut self, ch: char) {
        self.diff_search.push_char(ch);
        self.recompute_diff_matches(true);
    }

    pub fn diff_search_pop(&mut self) {
        self.diff_search.pop_char();
        self.recompute_diff_matches(true);
    }

    pub fn next_diff_match(&mut self) {
        if let Some(idx) = self.diff_search.next() {
            self.scroll = idx;
        }
    }

    pub fn prev_diff_match(&mut self) {
        if let Some(idx) = self.diff_search.prev() {
            self.scroll = idx;
        }
    }

    fn diff_line_count(&self) -> usize {
        self.hunks.iter().map(|h| 1 + h.lines.len()).sum()
    }

    fn max_diff_scroll(&self) -> usize {
        self.diff_line_count().saturating_sub(1)
    }

    pub fn diff_scroll_left(&mut self) {
        self.diff_scroll_x = self.diff_scroll_x.saturating_sub(4);
    }

    pub fn diff_scroll_right(&mut self) {
        self.diff_scroll_x = self.diff_scroll_x.saturating_add(4).min(u16::MAX as usize);
    }

    pub fn file_scroll_left(&mut self) {
        self.status_view.file_scroll_x = self.status_view.file_scroll_x.saturating_sub(4);
    }

    pub fn file_scroll_right(&mut self) {
        // Cap at the longest visible path's char width so we don't drift past
        // the last column of any rendered entry.
        let max = self
            .status_view
            .files
            .iter()
            .map(|f| f.path.chars().count())
            .max()
            .unwrap_or(0);
        self.status_view.file_scroll_x = self.status_view.file_scroll_x.saturating_add(4).min(max);
    }

    fn recompute_diff_matches(&mut self, scroll_to_match: bool) {
        self.diff_search.matches.clear();
        if self.diff_search.query.is_empty() {
            self.diff_search.cursor = 0;
            return;
        }
        let q = self.diff_search.query_lower.as_str();
        let mut flat_idx = 0usize;
        for hunk in &self.hunks {
            flat_idx += 1; // header line
            for line in &hunk.lines {
                if line.content.to_lowercase().contains(q) {
                    self.diff_search.matches.push(flat_idx);
                }
                flat_idx += 1;
            }
        }
        debug_assert!(
            self.diff_search.matches.windows(2).all(|w| w[0] < w[1]),
            "diff_search_matches must be sorted for binary_search to be correct"
        );
        if !self.diff_search.matches.is_empty() {
            self.diff_search.cursor = self
                .diff_search
                .cursor
                .min(self.diff_search.matches.len().saturating_sub(1));
            if scroll_to_match {
                self.scroll_to_diff_match();
            }
        } else {
            self.diff_search.cursor = 0;
        }
    }

    fn scroll_to_diff_match(&mut self) {
        if let Some(&idx) = self.diff_search.matches.get(self.diff_search.cursor) {
            self.scroll = idx;
        }
    }

    fn clamp_to_filtered(&mut self) {
        // Copy out the data we need so the immutable borrow ends before we
        // call the mutating reload below.
        let target = {
            let indices = self.filtered_indices();
            if indices.contains(&self.status_view.selected) {
                None
            } else {
                indices.first().copied()
            }
        };
        if let Some(first) = target {
            self.status_view.selected = first;
            self.reload_diff();
        }
    }

    /// Dispatches a navigation action to the appropriate log list (commit or file).
    /// Returns `true` if the action was handled (i.e. we are in Log mode).
    fn navigate_log_list(&mut self, commit_nav: fn(&mut Self), file_nav: fn(&mut Self)) -> bool {
        if self.mode != ViewMode::Log {
            return false;
        }
        if self.log_view.drill_down {
            file_nav(self);
        } else {
            commit_nav(self);
        }
        true
    }

    /// Move `selected` by `delta` positions within the active filter view.
    /// Handles both empty-query (full file list) and non-empty (filtered subset)
    /// cases uniformly.
    fn move_selected_in_filter(&mut self, delta: isize) {
        // Resolve the new selection in a scoped block so the borrow on
        // filtered_indices does not outlive the mutating reload below.
        let resolved = {
            let indices = self.filtered_indices();
            if indices.is_empty() {
                None
            } else {
                let pos = indices.iter().position(|&i| i == self.status_view.selected);
                let new_pos = match pos {
                    Some(p) => {
                        let last = indices.len() as isize - 1;
                        (p as isize + delta).clamp(0, last) as usize
                    }
                    None => 0,
                };
                Some((pos, new_pos, indices[new_pos]))
            }
        };
        if let Some((pos, new_pos, new_selected)) = resolved
            && (Some(new_pos) != pos || self.status_view.selected != new_selected)
        {
            self.status_view.selected = new_selected;
            self.reload_diff();
        }
    }

    pub fn select_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_select_up, Self::log_file_select_up) {
                    return;
                }
                self.move_selected_in_filter(-1);
            }
            Focus::DiffViewer => {
                if self.diff_pane_view == DiffPaneView::File {
                    self.file_view_scroll_up(1);
                } else {
                    self.scroll = self.scroll.saturating_sub(1);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn select_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_select_down, Self::log_file_select_down) {
                    return;
                }
                self.move_selected_in_filter(1);
            }
            Focus::DiffViewer => {
                if self.diff_pane_view == DiffPaneView::File {
                    self.file_view_scroll_down(1);
                } else {
                    self.scroll = self.scroll.saturating_add(1).min(self.max_diff_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_page_up, Self::log_file_page_up) {
                    return;
                }
                self.move_selected_in_filter(-(LIST_PAGE_SIZE as isize));
            }
            Focus::DiffViewer => {
                if self.diff_pane_view == DiffPaneView::File {
                    self.file_view_scroll_up(DIFF_PAGE_SIZE);
                } else {
                    self.scroll = self.scroll.saturating_sub(DIFF_PAGE_SIZE);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_page_down, Self::log_file_page_down) {
                    return;
                }
                self.move_selected_in_filter(LIST_PAGE_SIZE as isize);
            }
            Focus::DiffViewer => {
                if self.diff_pane_view == DiffPaneView::File {
                    self.file_view_scroll_down(DIFF_PAGE_SIZE);
                } else {
                    self.scroll = self
                        .scroll
                        .saturating_add(DIFF_PAGE_SIZE)
                        .min(self.max_diff_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }

    fn load_commit_diff_for_selected(&mut self) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => {
                self.clear_diff_state();
                self.log_view.diff_title.clear();
                return;
            }
        };
        let result = self.with_repo(|repo| load_commit_diff_with_repo(repo, oid));
        if let Err(e) = &result {
            tracing::debug!(error = %e, "failed to load commit diff");
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }

    fn reset_drill_down_state(&mut self) {
        self.log_view.drill_down = false;
        self.log_view.commit_files.clear();
        self.log_view.file_selected = 0;
    }

    pub fn log_drill_in(&mut self) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => return,
        };
        match self.with_repo(|repo| load_commit_files_with_repo(repo, oid)) {
            Ok(files) => {
                self.log_view.commit_files = files;
                self.log_view.file_selected = 0;
                self.log_view.drill_down = true;
                if self.log_view.commit_files.is_empty() {
                    self.clear_diff_state();
                    self.log_view.diff_title = title;
                } else {
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "failed to load commit files");
            }
        }
    }

    pub fn log_drill_out(&mut self) {
        self.reset_drill_down_state();
        self.load_commit_diff_for_selected();
    }

    pub fn log_file_select_up(&mut self) {
        if cursor_up(&mut self.log_view.file_selected, 1) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_select_down(&mut self) {
        if cursor_down(
            &mut self.log_view.file_selected,
            self.log_view.commit_files.len(),
            1,
        ) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_up(&mut self) {
        if cursor_up(&mut self.log_view.file_selected, LIST_PAGE_SIZE) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_down(&mut self) {
        if cursor_down(
            &mut self.log_view.file_selected,
            self.log_view.commit_files.len(),
            LIST_PAGE_SIZE,
        ) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    fn load_file_diff_for_log_file_selected(&mut self) {
        let Some((oid, short_id, commit_title)) = self
            .log_view
            .commits
            .get(self.log_view.selected)
            .map(|c| (c.oid, c.short_id.clone(), c.to_string()))
        else {
            self.clear_diff_state();
            self.log_view.diff_title.clear();
            return;
        };
        let Some(path) = self
            .log_view
            .commit_files
            .get(self.log_view.file_selected)
            .map(|f| f.path.clone())
        else {
            self.clear_diff_state();
            self.log_view.diff_title = commit_title;
            return;
        };
        let title = format!("{short_id} {path}");
        let result = self.with_repo(|repo| load_commit_file_diff_with_repo(repo, oid, &path));
        if let Err(e) = &result {
            tracing::debug!(error = %e, file = %path, "failed to load commit file diff");
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }

    pub fn toggle_mode(&mut self) {
        self.clear_diff_state();
        match self.mode {
            ViewMode::Status => {
                self.mode = ViewMode::Log;
                self.reset_drill_down_state();
                match self.with_repo(|repo| load_commit_log_with_repo(repo, COMMIT_LOG_LIMIT)) {
                    Ok(commits) => {
                        self.log_view.commits = commits;
                        self.log_view.selected = 0;
                        self.load_commit_diff_for_selected();
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to load commit log");
                        self.log_view.commits.clear();
                        self.log_view.selected = 0;
                        self.status = Some(format!("git error: {e}"));
                    }
                }
            }
            ViewMode::Log => {
                self.mode = ViewMode::Status;
                self.reset_drill_down_state();
                self.refresh_diff(true);
            }
        }
    }

    pub fn log_select_up(&mut self) {
        if cursor_up(&mut self.log_view.selected, 1) {
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_select_down(&mut self) {
        if cursor_down(&mut self.log_view.selected, self.log_view.commits.len(), 1) {
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_up(&mut self) {
        if cursor_up(&mut self.log_view.selected, LIST_PAGE_SIZE) {
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_down(&mut self) {
        if cursor_down(
            &mut self.log_view.selected,
            self.log_view.commits.len(),
            LIST_PAGE_SIZE,
        ) {
            self.load_commit_diff_for_selected();
        }
    }

    pub fn set_accent_index(&mut self, idx: usize) {
        // Normalize on entry so we never persist out-of-range indices to the
        // session file, even though `current_accent` would tolerate them.
        self.accent_idx = idx % crate::config::Accent::ALL.len();
    }

    pub fn cycle_accent(&mut self) {
        self.accent_idx = (self.accent_idx + 1) % crate::config::Accent::ALL.len();
    }

    pub fn current_accent(&self) -> ratatui::style::Color {
        crate::config::Accent::from_index(self.accent_idx).color()
    }

    pub fn cycle_focus_forward(&mut self) {
        if self.terminal.fullscreen {
            let len = self.terminal.panes.len();
            if len > 0 {
                self.terminal.active = (self.terminal.active + 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                self.focus = Focus::DiffViewer;
            }
            Focus::DiffViewer => {
                if !self.terminal.panes.is_empty() {
                    self.terminal.active = 0;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::FileList;
                }
            }
            Focus::Terminal => {
                if self.terminal.active + 1 < self.terminal.panes.len() {
                    self.terminal.active += 1;
                } else {
                    self.focus = Focus::FileList;
                }
            }
        }
    }

    pub fn cycle_focus_backward(&mut self) {
        if self.terminal.fullscreen {
            let len = self.terminal.panes.len();
            if len > 0 {
                self.terminal.active = (self.terminal.active + len - 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                if !self.terminal.panes.is_empty() {
                    self.terminal.active = self.terminal.panes.len() - 1;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
            Focus::DiffViewer => {
                self.focus = Focus::FileList;
            }
            Focus::Terminal => {
                if self.terminal.active > 0 {
                    self.terminal.active -= 1;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
        }
    }

    pub fn toggle_terminal_fullscreen(&mut self) {
        if !self.terminal.fullscreen && self.terminal.panes.is_empty() {
            return;
        }
        self.terminal.fullscreen = !self.terminal.fullscreen;
        if self.terminal.fullscreen {
            self.focus = Focus::Terminal;
        }
    }

    pub fn set_pending_session(&mut self, state: crate::session::SessionState) {
        self.pending_session = Some(state);
    }

    pub fn save_session(&self) -> crate::session::SessionState {
        crate::session::SessionState {
            focus: Some(self.focus),
            selected_file: self
                .status_view
                .files
                .get(self.status_view.selected)
                .map(|f| f.path.clone()),
            scroll: self.scroll,
            active_pane: self.terminal.active,
            terminal_fullscreen: self.terminal.fullscreen,
            mode: Some(self.mode),
            log_selected: self.log_view.selected,
            accent_idx: self.accent_idx,
            log_drill_down: self.log_view.drill_down,
            log_file_selected: self.log_view.file_selected,
        }
    }

    pub fn restore_session(&mut self, state: &crate::session::SessionState) {
        // Pane / focus / fullscreen restoration — independent of view mode.
        self.terminal.active = state
            .active_pane
            .min(self.terminal.panes.len().saturating_sub(1));
        if let Some(focus) = state.focus {
            if focus == Focus::Terminal && self.terminal.panes.is_empty() {
                self.focus = Focus::FileList;
            } else {
                self.focus = focus;
            }
        }
        self.terminal.fullscreen = state.terminal_fullscreen && !self.terminal.panes.is_empty();
        if self.terminal.fullscreen {
            self.focus = Focus::Terminal;
        }
        self.accent_idx = state.accent_idx;

        // Mode-specific diff/scroll restoration. We avoid loading a workdir diff
        // when the saved mode is Log — otherwise we'd waste a load and clamp the
        // scroll against the wrong diff length.
        match state.mode {
            Some(ViewMode::Log) => self.restore_log_session(state),
            _ => self.restore_status_session(state),
        }

        tracing::debug!(
            focus = ?state.focus,
            file = ?state.selected_file,
            scroll = state.scroll,
            mode = ?state.mode,
            drill = state.log_drill_down,
            "session restored"
        );
    }

    fn restore_status_session(&mut self, state: &crate::session::SessionState) {
        if let Some(path) = &state.selected_file
            && let Some(idx) = self.status_view.files.iter().position(|f| &f.path == path)
        {
            self.status_view.selected = idx;
            self.refresh_diff(true);
            self.scroll = state.scroll.min(self.max_diff_scroll());
        }
        // If the saved file is no longer present, leave selected/scroll as they
        // were after the initial snapshot — applying saved_scroll to a different
        // file would jump the user to an unrelated location.
    }

    fn restore_log_session(&mut self, state: &crate::session::SessionState) {
        let commits = match self.with_repo(|repo| load_commit_log_with_repo(repo, COMMIT_LOG_LIMIT))
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to restore commit log");
                return;
            }
        };
        self.log_view.commits = commits;
        self.log_view.selected = state
            .log_selected
            .min(self.log_view.commits.len().saturating_sub(1));
        self.mode = ViewMode::Log;

        if state.log_drill_down {
            self.restore_log_drill_down(state);
        } else {
            self.load_commit_diff_for_selected();
        }
        self.scroll = state.scroll.min(self.max_diff_scroll());
    }

    fn restore_log_drill_down(&mut self, state: &crate::session::SessionState) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => {
                self.load_commit_diff_for_selected();
                return;
            }
        };
        match self.with_repo(|repo| load_commit_files_with_repo(repo, oid)) {
            Ok(files) => {
                self.log_view.commit_files = files;
                self.log_view.drill_down = true;
                if self.log_view.commit_files.is_empty() {
                    self.log_view.file_selected = 0;
                    self.clear_diff_state();
                    self.log_view.diff_title = title;
                } else {
                    self.log_view.file_selected = state
                        .log_file_selected
                        .min(self.log_view.commit_files.len().saturating_sub(1));
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "failed to load drill-down commit files");
                self.load_commit_diff_for_selected();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::{ChangeStatus, DiffHunk, DiffLine, LineKind, load_commit_log};
    use crate::test_util::{make_repo, run_git};
    use std::path::Path;

    /// Build an inert SnapshotChannel for tests: real receiver, real stop
    /// sender, but no worker thread driving the receiver.
    ///
    /// Drops `_stop_rx` immediately on purpose: the only contract of `_stop_tx`
    /// is "dropped → worker observes disconnect". Since there is no worker
    /// here, nothing waits on either side, and dropping `_stop_rx` upfront
    /// keeps the helper's tuple shape minimal. If a future test ever spawns
    /// a real worker against this channel, it must keep `_stop_rx` alive.
    fn dummy_snapshot_channel() -> (SnapshotChannel, std::sync::mpsc::Sender<SnapshotMsg>) {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        (
            SnapshotChannel {
                rx,
                _stop_tx: stop_tx,
            },
            tx,
        )
    }

    fn app_with_files(files: Vec<&str>) -> App {
        let (snapshot, _tx) = dummy_snapshot_channel();
        let mut status_view = StatusView {
            files: files
                .into_iter()
                .map(|path| ChangedFile {
                    path: path.to_string(),
                    status: ChangeStatus::Modified,
                })
                .collect(),
            ..Default::default()
        };
        status_view.recompute_filter();
        App {
            mode: ViewMode::Status,
            status_view,
            hunks: Vec::new(),
            scroll: 0,
            diff_scroll_x: 0,
            focus: Focus::FileList,
            status: None,
            repo_path: ".".to_string(),
            log_view: LogView::default(),
            terminal: TerminalState::new(None, false),
            repo_input: RepoInput::default(),
            diff_search: DiffSearch::default(),
            diff_pane_view: DiffPaneView::default(),
            file_view: FileViewState::default(),
            accent_idx: 0,
            tracking: None,
            snapshot,
            pending_session: None,
            repo_cache: None,
        }
    }

    fn context_hunk(lines: &[&str]) -> DiffHunk {
        DiffHunk {
            header: "@@ -1 +1 @@".to_string(),
            lines: lines
                .iter()
                .map(|content| DiffLine {
                    kind: LineKind::Context,
                    content: (*content).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn selection_clamps_when_file_list_shrinks() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.status_view.selected = 2;
        app.status_view.files = vec![ChangedFile {
            path: "a.rs".to_string(),
            status: ChangeStatus::Modified,
        }];

        let selected_path = app.restore_selection(Some("c.rs"));

        assert_eq!(selected_path.as_deref(), Some("a.rs"));
        assert_eq!(app.status_view.selected, 0);
    }

    #[test]
    fn selection_prefers_same_path_after_refresh() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.status_view.selected = 1;
        app.status_view.files = vec![
            ChangedFile {
                path: "a.rs".to_string(),
                status: ChangeStatus::Modified,
            },
            ChangedFile {
                path: "c.rs".to_string(),
                status: ChangeStatus::Modified,
            },
            ChangedFile {
                path: "b.rs".to_string(),
                status: ChangeStatus::Modified,
            },
        ];

        let selected_path = app.restore_selection(Some("b.rs"));

        assert_eq!(selected_path.as_deref(), Some("b.rs"));
        assert_eq!(app.status_view.selected, 2);
    }

    #[test]
    fn diff_scroll_saturates_on_page_up() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.scroll = 3;

        app.page_up();

        assert_eq!(app.scroll, 0);
    }

    #[test]
    fn diff_scroll_clamps_at_last_line_on_select_down() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        // 1 hunk = header + 1 content line = 2 total lines, max_scroll = 1
        app.hunks = vec![context_hunk(&["x"])];
        app.scroll = 1; // already at max

        app.select_down();

        assert_eq!(app.scroll, 1, "scroll must not exceed last line index");
    }

    #[test]
    fn diff_scroll_clamps_at_last_line_on_page_down() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.hunks = vec![context_hunk(&["x"])];
        app.scroll = 0;

        app.page_down(); // +20, but max is 1

        assert_eq!(app.scroll, 1);
    }

    #[test]
    fn diff_scroll_handles_large_restored_offset() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.hunks = vec![context_hunk(&["x"])];
        app.scroll = usize::MAX;

        app.select_down();

        assert_eq!(app.scroll, 1);
    }

    #[test]
    fn diff_match_refresh_can_preserve_manual_scroll() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.hunks = vec![context_hunk(&["needle"])];
        app.diff_search.query = "needle".to_string();
        app.diff_search.query_lower = "needle".to_string();
        app.scroll = 7;

        app.recompute_diff_matches(false);

        assert_eq!(app.diff_search.matches, vec![1]);
        assert_eq!(app.scroll, 7);
    }

    #[test]
    fn diff_search_input_scrolls_to_first_match() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.hunks = vec![context_hunk(&["alpha", "needle"])];

        app.diff_search_push('n');

        assert_eq!(app.diff_search.matches, vec![2]);
        assert_eq!(app.scroll, 2);
    }

    #[test]
    fn terminal_scrollback_is_capped_at_screen_rows() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.active = 0;
        app.terminal.size = (3, 10);

        let mut parser = vt100::Parser::new(3, 10, SCROLLBACK_LINES);
        parser.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n");
        app.terminal.parsers.insert(1, parser);
        app.terminal.scroll.insert(1, 6);

        app.sync_terminal_scroll();

        // vt100 visible_rows() panics when scrollback_offset > screen rows, so we
        // cap offset at screen height to avoid the overflow.
        let actual = app.terminal.parsers.get(&1).unwrap().screen().scrollback();
        assert_eq!(actual, app.terminal.size.0 as usize);
    }

    #[test]
    fn switch_pane_moves_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![
            PaneInfo {
                id: 1,
                title: "shell 1".into(),
            },
            PaneInfo {
                id: 2,
                title: "shell 2".into(),
            },
        ];
        assert_eq!(app.focus, Focus::FileList);
        app.switch_pane(1);
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.terminal.active, 1);
    }

    #[test]
    fn switch_pane_ignores_out_of_range() {
        let mut app = app_with_files(vec![]);
        app.switch_pane(5);
        assert_eq!(app.terminal.active, 0);
    }

    #[test]
    fn toggle_fullscreen_switches_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        assert_eq!(app.focus, Focus::FileList);

        app.toggle_terminal_fullscreen();

        assert!(app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn toggle_fullscreen_noop_with_no_panes() {
        let mut app = app_with_files(vec![]);
        assert!(app.terminal.panes.is_empty());

        app.toggle_terminal_fullscreen();

        assert!(!app.terminal.fullscreen);
    }

    #[test]
    fn close_last_pane_exits_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.fullscreen = true;
        app.focus = Focus::Terminal;
        app.terminal.scroll.insert(1, 3);
        app.terminal.prompt_bufs.insert(1, "cargo test".to_string());
        app.terminal.parsers.insert(1, vt100::Parser::new(3, 10, 0));

        app.close_active_pane();

        assert!(!app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
        assert!(!app.terminal.scroll.contains_key(&1));
        assert!(!app.terminal.prompt_bufs.contains_key(&1));
        assert!(!app.terminal.parsers.contains_key(&1));
    }

    #[test]
    fn restore_session_restores_active_pane_even_when_focus_is_not_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![
            PaneInfo {
                id: 1,
                title: "shell 1".into(),
            },
            PaneInfo {
                id: 2,
                title: "shell 2".into(),
            },
        ];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            active_pane: 1,
            ..Default::default()
        });

        assert_eq!(app.focus, Focus::FileList);
        assert_eq!(app.terminal.active, 1);
    }

    #[test]
    fn restore_session_fullscreen_forces_terminal_focus() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            terminal_fullscreen: true,
            ..Default::default()
        });

        assert!(app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn restore_session_keeps_log_scroll_after_loading_commit_diff() {
        let (_dir, path) = make_repo();
        let file_path = Path::new(&path).join("a.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n",
        )
        .unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);

        let mut app = app_with_files(vec![]);
        app.repo_path = path;

        app.restore_session(&crate::session::SessionState {
            mode: Some(ViewMode::Log),
            scroll: 2,
            ..Default::default()
        });

        assert_eq!(app.mode, ViewMode::Log);
        assert!(!app.hunks.is_empty());
        assert_eq!(app.scroll, 2);
    }

    #[test]
    fn log_drill_in_clears_stale_diff_for_empty_commit() {
        let (_dir, path) = make_repo();
        run_git(&path, &["commit", "--allow-empty", "-m", "empty"]);

        let mut app = app_with_files(vec![]);
        app.repo_path = path.clone();
        app.mode = ViewMode::Log;
        app.log_view.commits = load_commit_log(&path, 1).unwrap();
        app.hunks = vec![context_hunk(&["stale"])];
        app.log_view.diff_title = "stale".to_string();

        app.log_drill_in();

        assert!(app.log_view.drill_down);
        assert!(app.log_view.commit_files.is_empty());
        assert!(app.hunks.is_empty());
        assert!(app.log_view.diff_title.contains("empty"));
    }

    #[test]
    fn successful_snapshot_preserves_terminal_status() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            status: Some("terminal error: backend unavailable".to_string()),
            snapshot,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(RepoSnapshot {
            files: Vec::new(),
            tracking: None,
        }))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(
            app.status.as_deref(),
            Some("terminal error: backend unavailable")
        );
    }

    #[test]
    fn successful_snapshot_clears_git_status() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            status: Some("git error: not a repo".to_string()),
            snapshot,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(RepoSnapshot {
            files: Vec::new(),
            tracking: None,
        }))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(app.status, None);
    }
}
