use crate::backend::BackendEvent;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use crate::git::diff::{ChangedFile, DiffHunk, RepoSnapshot, load_file_diff, load_snapshot};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

fn strip_escape_sequences(data: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < data.len() {
        match data[i] {
            0x1b => {
                i += 1;
                if i >= data.len() {
                    break;
                }
                match data[i] {
                    b'[' => {
                        // CSI: skip until final byte 0x40–0x7e
                        i += 1;
                        while i < data.len() && !(0x40..=0x7e).contains(&data[i]) {
                            i += 1;
                        }
                        i += 1;
                    }
                    b']' => {
                        // OSC: skip until BEL or ST
                        i += 1;
                        while i < data.len() {
                            if data[i] == 0x07 {
                                i += 1;
                                break;
                            }
                            if data[i] == 0x1b && i + 1 < data.len() && data[i + 1] == b'\\' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            b'\r' | b'\n' => {
                result.push(data[i] as char);
                i += 1;
            }
            0x20..=0x7e => {
                result.push(data[i] as char);
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    result
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
    pub files: Vec<ChangedFile>,
    pub selected: usize,
    pub hunks: Vec<DiffHunk>,
    pub scroll: usize,
    pub focus: Focus,
    pub status: Option<String>,
    pub repo_path: String,
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
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(0);
        let path = repo_path.clone();

        thread::spawn(move || {
            loop {
                let msg = match load_snapshot(&path) {
                    Ok(s) => SnapshotMsg::Ok(s),
                    Err(e) => SnapshotMsg::Err(e.to_string()),
                };
                if tx.send(msg).is_err() {
                    break;
                }
                // Sleep for 1s, but exit immediately if stop signal arrives or sender drops.
                match stop_rx.recv_timeout(Duration::from_millis(1000)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });

        let backend: Box<dyn TerminalBackend> = Box::new(PtyBackend::new(&repo_path));

        let mut app = App {
            files: Vec::new(),
            selected: 0,
            hunks: Vec::new(),
            scroll: 0,
            focus: Focus::FileList,
            status: None,
            repo_path,
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
                    self.parsers.remove(&pane);
                    self.terminal_panes.retain(|p| p.id != pane);
                    if self.active_pane >= self.terminal_panes.len()
                        && !self.terminal_panes.is_empty()
                    {
                        self.active_pane = self.terminal_panes.len() - 1;
                    } else if self.terminal_panes.is_empty() {
                        self.active_pane = 0;
                    }
                }
            }
        }
    }

    pub fn create_terminal_pane(&mut self) -> anyhow::Result<()> {
        let (rows, cols) = self.terminal_size;
        let backend = self
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        let id = backend.create_pane(rows.max(1), cols.max(1))?;
        let parser = vt100::Parser::new(rows.max(1), cols.max(1), 0);
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
        self.parsers.remove(&id);
        self.prompt_bufs.remove(&id);
        self.terminal_panes.remove(self.active_pane);
        if self.terminal_panes.is_empty() {
            self.active_pane = 0;
            self.focus = Focus::DiffViewer;
        } else {
            self.active_pane = self.active_pane.min(self.terminal_panes.len() - 1);
        }
    }

    pub fn change_repo(&mut self, new_path: String) {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(0);
        let path = new_path.clone();
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
        // Replacing _stop_tx drops the old sender, signaling the old thread to exit.
        self._stop_tx = stop_tx;
        self.rx = rx;
        self.repo_path = new_path.clone();
        if let Some(ref mut backend) = self.backend {
            backend.set_cwd(std::path::Path::new(&new_path));
        }
        tracing::info!(path = %new_path, "repo changed");
        self.files.clear();
        self.selected = 0;
        self.hunks.clear();
        self.scroll = 0;
        self.search_query.clear();
        self.search_active = false;
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
            if let Some(backend) = &mut self.backend {
                let _ = backend.send_input(id, data);
            }
            if self.prompt_log_enabled {
                self.buffer_prompt_input(id, data);
            }
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
                        self.recompute_diff_matches();
                    }
                }
                Err(_) => {
                    self.hunks = Vec::new();
                    self.diff_search_matches.clear();
                    self.diff_search_cursor = 0;
                    self.scroll = 0;
                }
            }
        } else {
            self.hunks = Vec::new();
            self.diff_search_matches.clear();
            self.diff_search_cursor = 0;
            self.scroll = 0;
        }
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
        self.recompute_diff_matches();
    }

    pub fn diff_search_pop(&mut self) {
        self.diff_search_query.pop();
        self.recompute_diff_matches();
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

    fn recompute_diff_matches(&mut self) {
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
        if !self.diff_search_matches.is_empty() {
            self.diff_search_cursor =
                self.diff_search_cursor.min(self.diff_search_matches.len() - 1);
            self.scroll_to_diff_match();
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
        if !indices.contains(&self.selected) && let Some(&first) = indices.first() {
            self.selected = first;
            self.reload_diff();
        }
    }

    pub fn select_up(&mut self) {
        match self.focus {
            Focus::FileList => {
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
                self.scroll += 1;
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if !self.search_query.is_empty() {
                    let indices = self.filtered_indices();
                    if let Some(pos) = indices.iter().position(|&i| i == self.selected) {
                        self.selected = indices[pos.saturating_sub(10)];
                        self.reload_diff();
                    }
                } else {
                    self.selected = self.selected.saturating_sub(10);
                    self.reload_diff();
                }
            }
            Focus::DiffViewer => {
                self.scroll = self.scroll.saturating_sub(20);
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                if !self.search_query.is_empty() {
                    let indices = self.filtered_indices();
                    if indices.is_empty() {
                        return;
                    }
                    if let Some(pos) = indices.iter().position(|&i| i == self.selected) {
                        self.selected = indices[(pos + 10).min(indices.len() - 1)];
                        self.reload_diff();
                    }
                } else if !self.files.is_empty() {
                    self.selected = (self.selected + 10).min(self.files.len() - 1);
                    self.reload_diff();
                }
            }
            Focus::DiffViewer => {
                self.scroll += 20;
            }
            Focus::Terminal => {}
        }
    }

    pub fn cycle_focus_forward(&mut self) {
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

    pub fn set_pending_session(&mut self, state: crate::session::SessionState) {
        self.pending_session = Some(state);
    }

    pub fn save_session(&self) -> crate::session::SessionState {
        crate::session::SessionState {
            focus: Some(format!("{:?}", self.focus)),
            selected_file: self.files.get(self.selected).map(|f| f.path.clone()),
            scroll: self.scroll,
            active_pane: self.active_pane,
        }
    }

    pub fn restore_session(&mut self, state: &crate::session::SessionState) {
        if let Some(path) = &state.selected_file {
            if let Some(idx) = self.files.iter().position(|f| &f.path == path) {
                self.selected = idx;
                self.refresh_diff(true);
            }
        }
        self.scroll = state.scroll;
        self.active_pane = self.active_pane.min(
            self.terminal_panes.len().saturating_sub(1),
        );
        if let Some(focus_str) = &state.focus {
            match focus_str.as_str() {
                "DiffViewer" => self.focus = Focus::DiffViewer,
                "Terminal" if !self.terminal_panes.is_empty() => {
                    self.focus = Focus::Terminal;
                    self.active_pane = state.active_pane.min(self.terminal_panes.len() - 1);
                }
                _ => self.focus = Focus::FileList,
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
    use crate::git::diff::ChangeStatus;

    fn app_with_files(files: Vec<&str>) -> App {
        let (_tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (_stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        App {
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
            rx,
            _stop_tx,
            backend: None,
            parsers: HashMap::new(),
            prompt_log_enabled: false,
            prompt_bufs: HashMap::new(),
            pending_session: None,
        }
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
    fn switch_pane_moves_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![
            PaneInfo { id: 1, title: "shell 1".into() },
            PaneInfo { id: 2, title: "shell 2".into() },
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
    fn successful_snapshot_preserves_terminal_status() {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (_stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        let mut app = App {
            files: Vec::new(),
            selected: 0,
            hunks: Vec::new(),
            scroll: 0,
            focus: Focus::FileList,

            status: Some("terminal error: backend unavailable".to_string()),
            repo_path: ".".to_string(),
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
            rx,
            _stop_tx,
            backend: None,
            parsers: HashMap::new(),
            prompt_log_enabled: false,
            prompt_bufs: HashMap::new(),
            pending_session: None,
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
            files: Vec::new(),
            selected: 0,
            hunks: Vec::new(),
            scroll: 0,
            focus: Focus::FileList,

            status: Some("git error: not a repo".to_string()),
            repo_path: ".".to_string(),
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
            rx,
            _stop_tx,
            pending_session: None,
            backend: None,
            parsers: HashMap::new(),
            prompt_log_enabled: false,
            prompt_bufs: HashMap::new(),
        };

        tx.send(SnapshotMsg::Ok(RepoSnapshot { files: Vec::new() }))
            .unwrap();
        app.poll_snapshot();

        assert_eq!(app.status, None);
    }
}
