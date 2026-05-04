use crate::backend::BackendEvent;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use crate::git::diff::{
    ChangedFile, CommitEntry, DiffHunk, RepoSnapshot, load_commit_diff, load_commit_file_diff,
    load_commit_files, load_commit_log, load_file_diff, load_snapshot,
};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

const SCROLLBACK_LINES: usize = 1000;
const LIST_PAGE_SIZE: usize = 10;
const DIFF_PAGE_SIZE: usize = 20;
const COMMIT_LOG_LIMIT: usize = 500;

fn spawn_snapshot_thread(repo_path: &str) -> (Receiver<SnapshotMsg>, SyncSender<()>) {
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
    (rx, stop_tx)
}

fn commit_diff_title(entry: &CommitEntry) -> String {
    format!("{} {}", entry.short_id, entry.summary)
}

fn strip_escape_sequences(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.peek().copied() {
                Some('[') => {
                    // CSI: skip until final byte 0x40–0x7e
                    chars.next();
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
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

pub struct App {
    pub mode: ViewMode,
    pub files: Vec<ChangedFile>,
    pub selected: usize,
    pub hunks: Vec<DiffHunk>,
    pub scroll: usize,
    pub focus: Focus,
    pub status: Option<String>,
    pub repo_path: String,
    pub commits: Vec<CommitEntry>,
    pub log_selected: usize,
    pub log_diff_title: String,
    pub log_drill_down: bool,
    pub log_commit_files: Vec<ChangedFile>,
    pub log_file_selected: usize,
    pub terminal_panes: Vec<PaneInfo>,
    pub active_pane: usize,
    pub terminal_size: (u16, u16),
    pub search_query: String,
    pub search_active: bool,
    pub repo_input_active: bool,
    pub repo_input_buf: String,
    pub diff_search_active: bool,
    pub diff_search_query: String,
    pub diff_search_matches: Vec<usize>,
    pub diff_search_cursor: usize,
    pub terminal_fullscreen: bool,
    pub terminal_scroll: HashMap<PaneId, usize>,
    rx: Receiver<SnapshotMsg>,
    // Dropping this sender signals the background thread to exit.
    _stop_tx: SyncSender<()>,
    backend: Option<Box<dyn TerminalBackend>>,
    parsers: HashMap<PaneId, vt100::Parser>,
    prompt_log_enabled: bool,
    prompt_bufs: HashMap<PaneId, String>,
    pending_session: Option<crate::session::SessionState>,
}

impl App {
    pub fn new(repo_path: String, prompt_log: bool) -> Self {
        let (rx, stop_tx) = spawn_snapshot_thread(&repo_path);

        let backend: Box<dyn TerminalBackend> = Box::new(PtyBackend::new(&repo_path));

        let mut app = App {
            mode: ViewMode::Status,
            files: Vec::new(),
            selected: 0,
            hunks: Vec::new(),
            scroll: 0,
            focus: Focus::FileList,
            status: None,
            repo_path,
            commits: Vec::new(),
            log_selected: 0,
            log_diff_title: String::new(),
            log_drill_down: false,
            log_commit_files: Vec::new(),
            log_file_selected: 0,
            terminal_panes: Vec::new(),
            active_pane: 0,
            terminal_size: (22, 78),
            search_query: String::new(),
            search_active: false,
            repo_input_active: false,
            repo_input_buf: String::new(),
            diff_search_active: false,
            diff_search_query: String::new(),
            diff_search_matches: Vec::new(),
            diff_search_cursor: 0,
            terminal_fullscreen: false,
            terminal_scroll: HashMap::new(),
            rx,
            _stop_tx: stop_tx,
            backend: Some(backend),
            parsers: HashMap::new(),
            prompt_log_enabled: prompt_log,
            prompt_bufs: HashMap::new(),
            pending_session: None,
        };

        app.ensure_initial_terminal();
        tracing::info!(repo = %app.repo_path, "nightcrow started");
        app
    }

    fn ensure_initial_terminal(&mut self) {
        if self.backend.is_none() {
            return;
        }

        if let Err(err) = self.create_terminal_pane() {
            self.status = Some(format!("terminal error: {err}"));
        }
    }

    pub fn poll_snapshot(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                SnapshotMsg::Ok(snapshot) => {
                    let previous_path = self.files.get(self.selected).map(|f| f.path.clone());
                    self.files = snapshot.files;

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
            .backend
            .as_mut()
            .map(|b| b.drain_events())
            .unwrap_or_default();

        for event in events {
            match event {
                BackendEvent::Output { pane, data } => {
                    if let Some(parser) = self.parsers.get_mut(&pane) {
                        parser.process(&data);
                    }
                }
                BackendEvent::Exited { pane } => {
                    self.remove_terminal_pane_state(pane);
                    self.terminal_panes.retain(|p| p.id != pane);
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
        let (rows, cols) = self.terminal_size;
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        let id = backend.create_pane(rows.max(1), cols.max(1))?;
        let parser = vt100::Parser::new(rows.max(1), cols.max(1), SCROLLBACK_LINES);
        self.parsers.insert(id, parser);
        self.terminal_panes.push(PaneInfo {
            id,
            title: "shell".to_string(),
        });
        self.active_pane = self.terminal_panes.len() - 1;
        tracing::info!(pane = id, "terminal pane opened");
        Ok(())
    }

    pub fn close_active_pane(&mut self) {
        let Some(info) = self.terminal_panes.get(self.active_pane) else {
            return;
        };
        let id = info.id;
        tracing::info!(pane = id, "terminal pane closed");
        if let Some(backend) = &mut self.backend {
            backend.destroy_pane(id);
        }
        self.remove_terminal_pane_state(id);
        self.terminal_panes.remove(self.active_pane);
        self.clamp_active_pane_after_removal();
    }

    fn remove_terminal_pane_state(&mut self, id: PaneId) {
        self.parsers.remove(&id);
        self.prompt_bufs.remove(&id);
        self.terminal_scroll.remove(&id);
    }

    fn clamp_active_pane_after_removal(&mut self) {
        if self.terminal_panes.is_empty() {
            self.active_pane = 0;
            self.focus = Focus::DiffViewer;
            self.terminal_fullscreen = false;
        } else {
            self.active_pane = self.active_pane.min(self.terminal_panes.len() - 1);
        }
    }

    pub fn change_repo(&mut self, new_path: String) {
        // Replacing _stop_tx drops the old sender, signaling the old thread to exit.
        let (rx, stop_tx) = spawn_snapshot_thread(&new_path);
        self._stop_tx = stop_tx;
        self.rx = rx;
        if let Some(ref mut backend) = self.backend {
            backend.set_cwd(std::path::Path::new(&new_path));
        }
        tracing::info!(path = %new_path, "repo changed");
        self.repo_path = new_path;
        self.mode = ViewMode::Status;
        self.files.clear();
        self.selected = 0;
        self.hunks.clear();
        self.scroll = 0;
        self.commits.clear();
        self.log_selected = 0;
        self.log_diff_title.clear();
        self.reset_drill_down_state();
        self.search_query.clear();
        self.search_active = false;
        self.clear_diff_search();
        self.status = None;
    }

    pub fn start_repo_input(&mut self) {
        self.repo_input_buf = self.repo_path.clone();
        self.repo_input_active = true;
    }

    pub fn cancel_repo_input(&mut self) {
        self.repo_input_active = false;
        self.repo_input_buf.clear();
    }

    pub fn confirm_repo_input(&mut self) {
        self.repo_input_active = false;
        let path = std::mem::take(&mut self.repo_input_buf);
        self.change_repo(path);
    }

    pub fn repo_input_push(&mut self, ch: char) {
        self.repo_input_buf.push(ch);
    }

    pub fn repo_input_pop(&mut self) {
        self.repo_input_buf.pop();
    }

    pub fn switch_pane(&mut self, idx: usize) {
        if idx < self.terminal_panes.len() {
            self.active_pane = idx;
            self.focus = Focus::Terminal;
        }
    }

    pub fn send_terminal_input(&mut self, data: &[u8]) {
        if let Some(info) = self.terminal_panes.get(self.active_pane) {
            let id = info.id;
            self.terminal_scroll.remove(&id);
            if let Some(backend) = &mut self.backend
                && let Err(e) = backend.send_input(id, data)
            {
                tracing::warn!("failed to send terminal input to pane {id}: {e}");
            }
            if self.prompt_log_enabled {
                self.buffer_prompt_input(id, data);
            }
        }
    }

    pub fn scroll_terminal_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(id) = self.active_pane_id() {
            let offset = self.terminal_scroll.entry(id).or_insert(0);
            *offset = offset.saturating_add(lines);
        }
    }

    pub fn scroll_terminal_down(&mut self, lines: usize) {
        if let Some(id) = self.active_pane_id()
            && let Some(entry) = self.terminal_scroll.get_mut(&id)
        {
            *entry = entry.saturating_sub(lines);
            if *entry == 0 {
                self.terminal_scroll.remove(&id);
            }
        }
    }

    pub fn is_terminal_scrolled(&self) -> bool {
        self.active_pane_id()
            .and_then(|id| self.terminal_scroll.get(&id))
            .is_some_and(|&v| v > 0)
    }

    pub fn sync_terminal_scroll(&mut self) {
        let Some(id) = self.active_pane_id() else {
            return;
        };
        let offset = self.terminal_scroll.get(&id).copied().unwrap_or(0);
        let actual = match self.parsers.get_mut(&id) {
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
            self.terminal_scroll.remove(&id);
        } else {
            self.terminal_scroll.insert(id, actual);
        }
    }

    fn buffer_prompt_input(&mut self, pane_id: PaneId, data: &[u8]) {
        let text = strip_escape_sequences(data);
        let buf = self.prompt_bufs.entry(pane_id).or_default();
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
        if self.terminal_size == (rows, cols) {
            return;
        }
        self.terminal_size = (rows, cols);
        let r = rows.max(1);
        let c = cols.max(1);
        for info in &self.terminal_panes {
            if let Some(backend) = &mut self.backend {
                backend.resize(info.id, r, c);
            }
            if let Some(parser) = self.parsers.get_mut(&info.id) {
                parser.set_size(r, c);
            }
        }
    }

    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.terminal_panes.get(self.active_pane).map(|p| p.id)
    }

    pub fn active_screen(&self) -> Option<&vt100::Screen> {
        let id = self.active_pane_id()?;
        self.parsers.get(&id).map(|p| p.screen())
    }

    pub fn reload_diff(&mut self) {
        self.refresh_diff(true);
    }

    fn refresh_diff(&mut self, reset_scroll: bool) {
        if self.mode == ViewMode::Log {
            return;
        }
        let previous_scroll = self.scroll;
        if let Some(file) = self.files.get(self.selected) {
            let path = file.path.clone();
            match load_file_diff(&self.repo_path, &path) {
                Ok(hunks) => {
                    self.hunks = hunks;
                    if reset_scroll {
                        self.scroll = 0;
                        self.diff_search_cursor = 0;
                    } else {
                        self.scroll = previous_scroll;
                    }
                    if !self.diff_search_query.is_empty() {
                        self.recompute_diff_matches(reset_scroll);
                    }
                }
                Err(e) => {
                    tracing::debug!(error = %e, file = %path, "failed to load diff");
                    self.clear_diff_state();
                }
            }
        } else {
            self.clear_diff_state();
        }
    }

    fn clear_diff_state(&mut self) {
        self.hunks.clear();
        self.diff_search_matches.clear();
        self.diff_search_cursor = 0;
        self.scroll = 0;
    }

    fn restore_selection(&mut self, previous_path: Option<&str>) -> Option<String> {
        if self.files.is_empty() {
            self.selected = 0;
            return None;
        }

        if let Some(path) = previous_path
            && let Some(index) = self.files.iter().position(|file| file.path == path)
        {
            self.selected = index;
            return Some(path.to_string());
        }

        self.selected = self.selected.min(self.files.len() - 1);
        self.files.get(self.selected).map(|file| file.path.clone())
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.search_query.is_empty() {
            return (0..self.files.len()).collect();
        }
        let q = self.search_query.to_lowercase();
        self.files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.path.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    pub fn start_search(&mut self) {
        self.search_active = true;
    }

    pub fn cancel_search(&mut self) {
        self.search_active = false;
        self.search_query.clear();
    }

    pub fn confirm_search(&mut self) {
        self.search_active = false;
    }

    pub fn search_push(&mut self, ch: char) {
        self.search_query.push(ch);
        self.clamp_to_filtered();
    }

    pub fn search_pop(&mut self) {
        self.search_query.pop();
        self.clamp_to_filtered();
    }

    pub fn start_diff_search(&mut self) {
        self.diff_search_active = true;
    }

    pub fn cancel_diff_search(&mut self) {
        self.clear_diff_search();
    }

    fn clear_diff_search(&mut self) {
        self.diff_search_active = false;
        self.diff_search_query.clear();
        self.diff_search_matches.clear();
        self.diff_search_cursor = 0;
    }

    pub fn confirm_diff_search(&mut self) {
        self.diff_search_active = false;
    }

    pub fn diff_search_push(&mut self, ch: char) {
        self.diff_search_query.push(ch);
        self.recompute_diff_matches(true);
    }

    pub fn diff_search_pop(&mut self) {
        self.diff_search_query.pop();
        self.recompute_diff_matches(true);
    }

    pub fn next_diff_match(&mut self) {
        if self.diff_search_matches.is_empty() {
            return;
        }
        self.diff_search_cursor = (self.diff_search_cursor + 1) % self.diff_search_matches.len();
        self.scroll_to_diff_match();
    }

    pub fn prev_diff_match(&mut self) {
        if self.diff_search_matches.is_empty() {
            return;
        }
        if self.diff_search_cursor == 0 {
            self.diff_search_cursor = self.diff_search_matches.len() - 1;
        } else {
            self.diff_search_cursor -= 1;
        }
        self.scroll_to_diff_match();
    }

    fn diff_line_count(&self) -> usize {
        self.hunks.iter().map(|h| 1 + h.lines.len()).sum()
    }

    fn max_diff_scroll(&self) -> usize {
        self.diff_line_count().saturating_sub(1)
    }

    fn recompute_diff_matches(&mut self, scroll_to_match: bool) {
        self.diff_search_matches.clear();
        if self.diff_search_query.is_empty() {
            self.diff_search_cursor = 0;
            return;
        }
        let q = self.diff_search_query.to_lowercase();
        let mut flat_idx = 0usize;
        for hunk in &self.hunks {
            flat_idx += 1; // header line
            for line in &hunk.lines {
                if line.content.to_lowercase().contains(&q) {
                    self.diff_search_matches.push(flat_idx);
                }
                flat_idx += 1;
            }
        }
        debug_assert!(
            self.diff_search_matches.windows(2).all(|w| w[0] < w[1]),
            "diff_search_matches must be sorted for binary_search to be correct"
        );
        if !self.diff_search_matches.is_empty() {
            self.diff_search_cursor = self
                .diff_search_cursor
                .min(self.diff_search_matches.len() - 1);
            if scroll_to_match {
                self.scroll_to_diff_match();
            }
        } else {
            self.diff_search_cursor = 0;
        }
    }

    fn scroll_to_diff_match(&mut self) {
        if let Some(&idx) = self.diff_search_matches.get(self.diff_search_cursor) {
            self.scroll = idx;
        }
    }

    fn clamp_to_filtered(&mut self) {
        let indices = self.filtered_indices();
        if !indices.contains(&self.selected)
            && let Some(&first) = indices.first()
        {
            self.selected = first;
            self.reload_diff();
        }
    }

    /// Dispatches a navigation action to the appropriate log list (commit or file).
    /// Returns `true` if the action was handled (i.e. we are in Log mode).
    fn navigate_log_list(
        &mut self,
        commit_nav: fn(&mut Self),
        file_nav: fn(&mut Self),
    ) -> bool {
        if self.mode != ViewMode::Log {
            return false;
        }
        if self.log_drill_down {
            file_nav(self);
        } else {
            commit_nav(self);
        }
        true
    }

    pub fn select_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.navigate_log_list(Self::log_select_up, Self::log_file_select_up) {
                    return;
                }
                if !self.search_query.is_empty() {
                    let indices = self.filtered_indices();
                    if let Some(pos) = indices.iter().position(|&i| i == self.selected)
                        && pos > 0
                    {
                        self.selected = indices[pos - 1];
                        self.reload_diff();
                    }
                } else if self.selected > 0 {
                    self.selected -= 1;
                    self.reload_diff();
                }
            }
            Focus::DiffViewer => {
                self.scroll = self.scroll.saturating_sub(1);
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
                if !self.search_query.is_empty() {
                    let indices = self.filtered_indices();
                    if let Some(pos) = indices.iter().position(|&i| i == self.selected)
                        && pos + 1 < indices.len()
                    {
                        self.selected = indices[pos + 1];
                        self.reload_diff();
                    }
                } else if !self.files.is_empty() && self.selected < self.files.len() - 1 {
                    self.selected += 1;
                    self.reload_diff();
                }
            }
            Focus::DiffViewer => {
                self.scroll = self.scroll.saturating_add(1).min(self.max_diff_scroll());
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
                if !self.search_query.is_empty() {
                    let indices = self.filtered_indices();
                    if let Some(pos) = indices.iter().position(|&i| i == self.selected) {
                        self.selected = indices[pos.saturating_sub(LIST_PAGE_SIZE)];
                        self.reload_diff();
                    }
                } else {
                    self.selected = self.selected.saturating_sub(LIST_PAGE_SIZE);
                    self.reload_diff();
                }
            }
            Focus::DiffViewer => {
                self.scroll = self.scroll.saturating_sub(DIFF_PAGE_SIZE);
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
                if !self.search_query.is_empty() {
                    let indices = self.filtered_indices();
                    if indices.is_empty() {
                        return;
                    }
                    if let Some(pos) = indices.iter().position(|&i| i == self.selected) {
                        self.selected = indices[(pos + LIST_PAGE_SIZE).min(indices.len() - 1)];
                        self.reload_diff();
                    }
                } else if !self.files.is_empty() {
                    self.selected = (self.selected + LIST_PAGE_SIZE).min(self.files.len() - 1);
                    self.reload_diff();
                }
            }
            Focus::DiffViewer => {
                self.scroll = self
                    .scroll
                    .saturating_add(DIFF_PAGE_SIZE)
                    .min(self.max_diff_scroll());
            }
            Focus::Terminal => {}
        }
    }

    fn load_commit_diff_for_selected(&mut self) {
        if let Some(entry) = self.commits.get(self.log_selected) {
            let oid = entry.oid;
            let title = commit_diff_title(entry);
            match load_commit_diff(&self.repo_path, oid) {
                Ok(hunks) => {
                    self.hunks = hunks;
                    self.scroll = 0;
                    self.log_diff_title = title;
                    if !self.diff_search_query.is_empty() {
                        self.recompute_diff_matches(true);
                    }
                }
                Err(e) => {
                    tracing::debug!(error = %e, "failed to load commit diff");
                    self.clear_diff_state();
                    self.log_diff_title = title;
                }
            }
        } else {
            self.clear_diff_state();
            self.log_diff_title.clear();
        }
    }

    fn reset_drill_down_state(&mut self) {
        self.log_drill_down = false;
        self.log_commit_files.clear();
        self.log_file_selected = 0;
    }

    pub fn log_drill_in(&mut self) {
        let Some(entry) = self.commits.get(self.log_selected) else {
            return;
        };
        let oid = entry.oid;
        let title = commit_diff_title(entry);
        match load_commit_files(&self.repo_path, oid) {
            Ok(files) => {
                self.log_commit_files = files;
                self.log_file_selected = 0;
                self.log_drill_down = true;
                if self.log_commit_files.is_empty() {
                    self.clear_diff_state();
                    self.log_diff_title = title;
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
        if self.log_file_selected > 0 {
            self.log_file_selected -= 1;
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_select_down(&mut self) {
        if !self.log_commit_files.is_empty()
            && self.log_file_selected < self.log_commit_files.len() - 1
        {
            self.log_file_selected += 1;
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_up(&mut self) {
        if !self.log_commit_files.is_empty() {
            self.log_file_selected = self.log_file_selected.saturating_sub(LIST_PAGE_SIZE);
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_down(&mut self) {
        if !self.log_commit_files.is_empty() {
            self.log_file_selected =
                (self.log_file_selected + LIST_PAGE_SIZE).min(self.log_commit_files.len() - 1);
            self.load_file_diff_for_log_file_selected();
        }
    }

    fn load_file_diff_for_log_file_selected(&mut self) {
        let Some(commit_entry) = self.commits.get(self.log_selected) else {
            self.clear_diff_state();
            self.log_diff_title.clear();
            return;
        };
        let oid = commit_entry.oid;
        let commit_title = commit_diff_title(commit_entry);
        let Some(file) = self.log_commit_files.get(self.log_file_selected) else {
            self.clear_diff_state();
            self.log_diff_title = commit_title;
            return;
        };
        let path = file.path.clone();
        let title = format!("{} {}", commit_entry.short_id, path);
        match load_commit_file_diff(&self.repo_path, oid, &path) {
            Ok(hunks) => {
                self.hunks = hunks;
                self.scroll = 0;
                self.log_diff_title = title;
                if !self.diff_search_query.is_empty() {
                    self.recompute_diff_matches(true);
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, file = %path, "failed to load commit file diff");
                self.clear_diff_state();
                self.log_diff_title = title;
            }
        }
    }

    pub fn toggle_mode(&mut self) {
        self.clear_diff_state();
        match self.mode {
            ViewMode::Status => {
                self.mode = ViewMode::Log;
                self.reset_drill_down_state();
                match load_commit_log(&self.repo_path, COMMIT_LOG_LIMIT) {
                    Ok(commits) => {
                        self.commits = commits;
                        self.log_selected = 0;
                        self.load_commit_diff_for_selected();
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to load commit log");
                        self.commits.clear();
                        self.log_selected = 0;
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
        if self.log_selected > 0 {
            self.log_selected -= 1;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_select_down(&mut self) {
        if !self.commits.is_empty() && self.log_selected < self.commits.len() - 1 {
            self.log_selected += 1;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_up(&mut self) {
        self.log_selected = self.log_selected.saturating_sub(LIST_PAGE_SIZE);
        self.load_commit_diff_for_selected();
    }

    pub fn log_page_down(&mut self) {
        if !self.commits.is_empty() {
            self.log_selected = (self.log_selected + LIST_PAGE_SIZE).min(self.commits.len() - 1);
            self.load_commit_diff_for_selected();
        }
    }

    pub fn cycle_focus_forward(&mut self) {
        if self.terminal_fullscreen {
            let len = self.terminal_panes.len();
            if len > 0 {
                self.active_pane = (self.active_pane + 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                self.focus = Focus::DiffViewer;
            }
            Focus::DiffViewer => {
                if !self.terminal_panes.is_empty() {
                    self.active_pane = 0;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::FileList;
                }
            }
            Focus::Terminal => {
                if self.active_pane + 1 < self.terminal_panes.len() {
                    self.active_pane += 1;
                } else {
                    self.focus = Focus::FileList;
                }
            }
        }
    }

    pub fn cycle_focus_backward(&mut self) {
        if self.terminal_fullscreen {
            let len = self.terminal_panes.len();
            if len > 0 {
                self.active_pane = (self.active_pane + len - 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                if !self.terminal_panes.is_empty() {
                    self.active_pane = self.terminal_panes.len() - 1;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
            Focus::DiffViewer => {
                self.focus = Focus::FileList;
            }
            Focus::Terminal => {
                if self.active_pane > 0 {
                    self.active_pane -= 1;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
        }
    }

    pub fn toggle_terminal_fullscreen(&mut self) {
        if !self.terminal_fullscreen && self.terminal_panes.is_empty() {
            return;
        }
        self.terminal_fullscreen = !self.terminal_fullscreen;
        if self.terminal_fullscreen {
            self.focus = Focus::Terminal;
        }
    }

    pub fn set_pending_session(&mut self, state: crate::session::SessionState) {
        self.pending_session = Some(state);
    }

    pub fn save_session(&self) -> crate::session::SessionState {
        crate::session::SessionState {
            focus: Some(self.focus),
            selected_file: self.files.get(self.selected).map(|f| f.path.clone()),
            scroll: self.scroll,
            active_pane: self.active_pane,
            terminal_fullscreen: self.terminal_fullscreen,
            mode: Some(self.mode),
            log_selected: self.log_selected,
        }
    }

    pub fn restore_session(&mut self, state: &crate::session::SessionState) {
        let saved_scroll = state.scroll;
        if let Some(path) = &state.selected_file
            && let Some(idx) = self.files.iter().position(|f| &f.path == path)
        {
            self.selected = idx;
            self.refresh_diff(true);
        }
        self.scroll = saved_scroll.min(self.max_diff_scroll());
        self.active_pane = state
            .active_pane
            .min(self.terminal_panes.len().saturating_sub(1));
        if let Some(focus) = state.focus {
            if focus == Focus::Terminal && self.terminal_panes.is_empty() {
                self.focus = Focus::FileList;
            } else {
                self.focus = focus;
            }
        }
        self.terminal_fullscreen = state.terminal_fullscreen && !self.terminal_panes.is_empty();
        if state.mode == Some(ViewMode::Log) {
            match load_commit_log(&self.repo_path, COMMIT_LOG_LIMIT) {
                Ok(commits) => {
                    self.commits = commits;
                    self.log_selected =
                        state.log_selected.min(self.commits.len().saturating_sub(1));
                    self.mode = ViewMode::Log;
                    self.load_commit_diff_for_selected();
                    self.scroll = saved_scroll.min(self.max_diff_scroll());
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to restore commit log");
                }
            }
        }
        tracing::debug!(
            focus = ?state.focus,
            file = ?state.selected_file,
            scroll = state.scroll,
            "session restored"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::{ChangeStatus, DiffHunk, DiffLine, LineKind, load_commit_log};
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    fn app_with_files(files: Vec<&str>) -> App {
        let (_tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (_stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        App {
            mode: ViewMode::Status,
            files: files
                .into_iter()
                .map(|path| ChangedFile {
                    path: path.to_string(),
                    status: ChangeStatus::Modified,
                })
                .collect(),
            selected: 0,
            hunks: Vec::new(),
            scroll: 0,
            focus: Focus::FileList,
            status: None,
            repo_path: ".".to_string(),
            commits: Vec::new(),
            log_selected: 0,
            log_diff_title: String::new(),
            log_drill_down: false,
            log_commit_files: Vec::new(),
            log_file_selected: 0,
            terminal_panes: Vec::new(),
            active_pane: 0,
            terminal_size: (22, 78),
            search_query: String::new(),
            search_active: false,
            repo_input_active: false,
            repo_input_buf: String::new(),
            diff_search_active: false,
            diff_search_query: String::new(),
            diff_search_matches: Vec::new(),
            diff_search_cursor: 0,
            terminal_fullscreen: false,
            terminal_scroll: HashMap::new(),
            rx,
            _stop_tx,
            backend: None,
            parsers: HashMap::new(),
            prompt_log_enabled: false,
            prompt_bufs: HashMap::new(),
            pending_session: None,
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

    fn run_git(repo_path: &str, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo_path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn make_repo() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        run_git(&path, &["init"]);
        run_git(&path, &["config", "user.email", "t@t.com"]);
        run_git(&path, &["config", "user.name", "T"]);
        (dir, path)
    }

    #[test]
    fn selection_clamps_when_file_list_shrinks() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.selected = 2;
        app.files = vec![ChangedFile {
            path: "a.rs".to_string(),
            status: ChangeStatus::Modified,
        }];

        let selected_path = app.restore_selection(Some("c.rs"));

        assert_eq!(selected_path.as_deref(), Some("a.rs"));
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn selection_prefers_same_path_after_refresh() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.selected = 1;
        app.files = vec![
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
        assert_eq!(app.selected, 2);
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
        app.diff_search_query = "needle".to_string();
        app.scroll = 7;

        app.recompute_diff_matches(false);

        assert_eq!(app.diff_search_matches, vec![1]);
        assert_eq!(app.scroll, 7);
    }

    #[test]
    fn diff_search_input_scrolls_to_first_match() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.hunks = vec![context_hunk(&["alpha", "needle"])];

        app.diff_search_push('n');

        assert_eq!(app.diff_search_matches, vec![2]);
        assert_eq!(app.scroll, 2);
    }

    #[test]
    fn terminal_scrollback_is_capped_at_screen_rows() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.active_pane = 0;
        app.terminal_size = (3, 10);

        let mut parser = vt100::Parser::new(3, 10, SCROLLBACK_LINES);
        parser.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n");
        app.parsers.insert(1, parser);
        app.terminal_scroll.insert(1, 6);

        app.sync_terminal_scroll();

        // vt100 visible_rows() panics when scrollback_offset > screen rows, so we
        // cap offset at screen height to avoid the overflow.
        let actual = app.parsers.get(&1).unwrap().screen().scrollback();
        assert_eq!(actual, app.terminal_size.0 as usize);
    }

    #[test]
    fn switch_pane_moves_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![
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
        assert_eq!(app.active_pane, 1);
    }

    #[test]
    fn switch_pane_ignores_out_of_range() {
        let mut app = app_with_files(vec![]);
        app.switch_pane(5);
        assert_eq!(app.active_pane, 0);
    }

    #[test]
    fn toggle_fullscreen_switches_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        assert_eq!(app.focus, Focus::FileList);

        app.toggle_terminal_fullscreen();

        assert!(app.terminal_fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn toggle_fullscreen_noop_with_no_panes() {
        let mut app = app_with_files(vec![]);
        assert!(app.terminal_panes.is_empty());

        app.toggle_terminal_fullscreen();

        assert!(!app.terminal_fullscreen);
    }

    #[test]
    fn close_last_pane_exits_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal_fullscreen = true;
        app.focus = Focus::Terminal;
        app.terminal_scroll.insert(1, 3);
        app.prompt_bufs.insert(1, "cargo test".to_string());
        app.parsers.insert(1, vt100::Parser::new(3, 10, 0));

        app.close_active_pane();

        assert!(!app.terminal_fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
        assert!(!app.terminal_scroll.contains_key(&1));
        assert!(!app.prompt_bufs.contains_key(&1));
        assert!(!app.parsers.contains_key(&1));
    }

    #[test]
    fn restore_session_restores_active_pane_even_when_focus_is_not_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![
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
        assert_eq!(app.active_pane, 1);
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
        app.commits = load_commit_log(&path, 1).unwrap();
        app.hunks = vec![context_hunk(&["stale"])];
        app.log_diff_title = "stale".to_string();

        app.log_drill_in();

        assert!(app.log_drill_down);
        assert!(app.log_commit_files.is_empty());
        assert!(app.hunks.is_empty());
        assert!(app.log_diff_title.contains("empty"));
    }

    #[test]
    fn successful_snapshot_preserves_terminal_status() {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (_stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        let mut app = App {
            status: Some("terminal error: backend unavailable".to_string()),
            rx,
            _stop_tx,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(RepoSnapshot { files: Vec::new() }))
            .unwrap();
        app.poll_snapshot();

        assert_eq!(
            app.status.as_deref(),
            Some("terminal error: backend unavailable")
        );
    }

    #[test]
    fn successful_snapshot_clears_git_status() {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (_stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        let mut app = App {
            status: Some("git error: not a repo".to_string()),
            rx,
            _stop_tx,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(RepoSnapshot { files: Vec::new() }))
            .unwrap();
        app.poll_snapshot();

        assert_eq!(app.status, None);
    }
}
