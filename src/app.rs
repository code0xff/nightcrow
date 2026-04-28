use crate::backend::BackendEvent;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use crate::git::diff::{ChangedFile, DiffHunk, RepoSnapshot, load_file_diff, load_snapshot};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
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
    pub last_upper_focus: Focus,
    pub status: Option<String>,
    pub repo_path: String,
    pub terminal_panes: Vec<PaneInfo>,
    pub active_pane: usize,
    pub terminal_size: (u16, u16),
    rx: Receiver<SnapshotMsg>,
    // Dropping this sender signals the background thread to exit.
    _stop_tx: SyncSender<()>,
    backend: Option<Box<dyn TerminalBackend>>,
    parsers: HashMap<PaneId, vt100::Parser>,
}

impl App {
    pub fn new(repo_path: String) -> Self {
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

        let backend: Box<dyn TerminalBackend> = Box::new(PtyBackend::new());

        let mut app = App {
            files: Vec::new(),
            selected: 0,
            hunks: Vec::new(),
            scroll: 0,
            focus: Focus::FileList,
            last_upper_focus: Focus::FileList,
            status: None,
            repo_path,
            terminal_panes: Vec::new(),
            active_pane: 0,
            terminal_size: (22, 78),
            rx,
            _stop_tx: stop_tx,
            backend: Some(backend),
            parsers: HashMap::new(),
        };

        app.ensure_initial_terminal();
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
                }
                SnapshotMsg::Err(e) => {
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
        Ok(())
    }

    pub fn switch_pane(&mut self, idx: usize) {
        if idx < self.terminal_panes.len() {
            self.active_pane = idx;
        }
    }

    pub fn send_terminal_input(&mut self, data: &[u8]) {
        if let Some(info) = self.terminal_panes.get(self.active_pane) {
            let id = info.id;
            if let Some(backend) = &mut self.backend {
                let _ = backend.send_input(id, data);
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
                    } else {
                        self.scroll = previous_scroll;
                    }
                }
                Err(_) => {
                    self.hunks = Vec::new();
                    self.scroll = 0;
                }
            }
        } else {
            self.hunks = Vec::new();
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

    pub fn select_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                if self.selected > 0 {
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
                if !self.files.is_empty() && self.selected < self.files.len() - 1 {
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
                self.selected = self.selected.saturating_sub(10);
                self.reload_diff();
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
                if !self.files.is_empty() {
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

    /// Tab: cycle forward FileList → DiffViewer → Terminal[0] → Terminal[1] → … → FileList
    pub fn cycle_focus_next(&mut self) {
        let pane_count = self.terminal_panes.len();
        match self.focus {
            Focus::FileList => {
                self.last_upper_focus = Focus::FileList;
                self.focus = Focus::DiffViewer;
            }
            Focus::DiffViewer => {
                self.last_upper_focus = Focus::DiffViewer;
                if pane_count > 0 {
                    self.active_pane = 0;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::FileList;
                }
            }
            Focus::Terminal => {
                if self.active_pane + 1 < pane_count {
                    self.active_pane += 1;
                } else {
                    self.focus = Focus::FileList;
                }
            }
        }
    }

    /// BackTab: cycle backward FileList → Terminal[last] → … → Terminal[0] → DiffViewer → FileList
    pub fn cycle_focus_prev(&mut self) {
        let pane_count = self.terminal_panes.len();
        match self.focus {
            Focus::FileList => {
                if pane_count > 0 {
                    self.active_pane = pane_count - 1;
                    self.last_upper_focus = Focus::FileList;
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
                    self.last_upper_focus = Focus::DiffViewer;
                    self.focus = Focus::DiffViewer;
                }
            }
        }
    }

    pub fn toggle_upper_focus(&mut self) {
        self.focus = match self.focus {
            Focus::FileList => Focus::DiffViewer,
            Focus::DiffViewer => Focus::FileList,
            Focus::Terminal => self.last_upper_focus,
        };

        if self.focus != Focus::Terminal {
            self.last_upper_focus = self.focus;
        }
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
            last_upper_focus: Focus::FileList,
            status: None,
            repo_path: ".".to_string(),
            terminal_panes: Vec::new(),
            active_pane: 0,
            terminal_size: (22, 78),
            rx,
            _stop_tx,
            backend: None,
            parsers: HashMap::new(),
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
    fn tab_cycles_panels_without_terminal_panes() {
        let mut app = app_with_files(vec![]);
        assert_eq!(app.focus, Focus::FileList);
        app.cycle_focus_next();
        assert_eq!(app.focus, Focus::DiffViewer);
        app.cycle_focus_next();
        assert_eq!(app.focus, Focus::FileList);
    }

    #[test]
    fn tab_cycles_through_terminal_panes() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![
            PaneInfo { id: 1, title: "shell 1".into() },
            PaneInfo { id: 2, title: "shell 2".into() },
        ];
        app.cycle_focus_next();
        assert_eq!(app.focus, Focus::DiffViewer);
        app.cycle_focus_next();
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.active_pane, 0);
        app.cycle_focus_next();
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.active_pane, 1);
        app.cycle_focus_next();
        assert_eq!(app.focus, Focus::FileList);
    }

    #[test]
    fn backtab_cycles_focus_in_reverse() {
        let mut app = app_with_files(vec![]);
        app.terminal_panes = vec![PaneInfo { id: 1, title: "shell 1".into() }];
        app.cycle_focus_prev();
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.active_pane, 0);
        app.cycle_focus_prev();
        assert_eq!(app.focus, Focus::DiffViewer);
        app.cycle_focus_prev();
        assert_eq!(app.focus, Focus::FileList);
    }

    #[test]
    fn toggle_upper_focus_switches_file_list_and_diff_viewer() {
        let mut app = app_with_files(vec![]);

        app.toggle_upper_focus();
        assert_eq!(app.focus, Focus::DiffViewer);
        assert_eq!(app.last_upper_focus, Focus::DiffViewer);

        app.toggle_upper_focus();
        assert_eq!(app.focus, Focus::FileList);
        assert_eq!(app.last_upper_focus, Focus::FileList);
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
            last_upper_focus: Focus::FileList,
            status: Some("terminal error: backend unavailable".to_string()),
            repo_path: ".".to_string(),
            terminal_panes: Vec::new(),
            active_pane: 0,
            terminal_size: (22, 78),
            rx,
            _stop_tx,
            backend: None,
            parsers: HashMap::new(),
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
            last_upper_focus: Focus::FileList,
            status: Some("git error: not a repo".to_string()),
            repo_path: ".".to_string(),
            terminal_panes: Vec::new(),
            active_pane: 0,
            terminal_size: (22, 78),
            rx,
            _stop_tx,
            backend: None,
            parsers: HashMap::new(),
        };

        tx.send(SnapshotMsg::Ok(RepoSnapshot { files: Vec::new() }))
            .unwrap();
        app.poll_snapshot();

        assert_eq!(app.status, None);
    }
}
