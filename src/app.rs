use crate::git::diff::{ChangedFile, DiffHunk, RepoSnapshot, load_file_diff, load_snapshot};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    FileList,
    DiffViewer,
}

pub enum SnapshotMsg {
    Ok(RepoSnapshot),
    Err(String),
}

pub struct App {
    pub files: Vec<ChangedFile>,
    pub selected: usize,
    pub hunks: Vec<DiffHunk>,
    pub scroll: usize,
    pub focus: Focus,
    pub status: Option<String>,
    pub repo_path: String,
    rx: Receiver<SnapshotMsg>,
    // Dropping this sender signals the background thread to exit.
    _stop_tx: SyncSender<()>,
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

        App {
            files: Vec::new(),
            selected: 0,
            hunks: Vec::new(),
            scroll: 0,
            focus: Focus::FileList,
            status: None,
            repo_path,
            rx,
            _stop_tx: stop_tx,
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
                    self.status = None;
                }
                SnapshotMsg::Err(e) => {
                    self.status = Some(format!("git error: {e}"));
                }
            }
        }
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
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::FileList => Focus::DiffViewer,
            Focus::DiffViewer => Focus::FileList,
        };
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
            rx,
            _stop_tx,
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
}
