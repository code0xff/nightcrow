use crate::git::diff::{ChangedFile, DiffHunk, RepoSnapshot, load_file_diff, load_snapshot};
use std::sync::mpsc::{self, Receiver};
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
}

impl App {
    pub fn new(repo_path: String) -> Self {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let path = repo_path.clone();

        thread::spawn(move || loop {
            let msg = match load_snapshot(&path) {
                Ok(s) => SnapshotMsg::Ok(s),
                Err(e) => SnapshotMsg::Err(e.to_string()),
            };
            let _ = tx.send(msg);
            thread::sleep(Duration::from_millis(1000));
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
        }
    }

    pub fn poll_snapshot(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                SnapshotMsg::Ok(snapshot) => {
                    let prev_len = self.files.len();
                    self.files = snapshot.files;
                    // Clamp selection if list shrank
                    if !self.files.is_empty() {
                        self.selected = self.selected.min(self.files.len() - 1);
                    } else {
                        self.selected = 0;
                    }
                    // Reload diff if file list changed
                    if self.files.len() != prev_len {
                        self.reload_diff();
                    }
                    self.status = None;
                }
                SnapshotMsg::Err(e) => {
                    self.status = Some(format!("git error: {e}"));
                }
            }
        }
    }

    pub fn reload_diff(&mut self) {
        if let Some(file) = self.files.get(self.selected) {
            let path = file.path.clone();
            match load_file_diff(&self.repo_path, &path) {
                Ok(hunks) => {
                    self.hunks = hunks;
                    self.scroll = 0;
                }
                Err(_) => {
                    self.hunks = Vec::new();
                }
            }
        } else {
            self.hunks = Vec::new();
        }
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
