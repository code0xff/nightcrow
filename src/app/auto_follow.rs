use super::{App, Focus, ViewMode};
use std::time::{Duration, Instant, SystemTime};

impl App {
    pub(crate) fn mark_user_navigated(&mut self) {
        self.git.view.auto_follow.last_manual_nav_at = Some(Instant::now());
        self.git.view.auto_follow.followed_path = None;
    }

    // Returns `true` when selection changed; caller refreshes the diff.
    pub(crate) fn try_auto_follow(&mut self) -> bool {
        if !self.git.agent_indicator.enabled || !self.git.agent_indicator.auto_follow {
            return false;
        }
        if self.focus != Focus::FileList || self.git.view.mode != ViewMode::Status {
            return false;
        }
        let idle = match self.git.view.auto_follow.last_manual_nav_at {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_secs(2),
        };
        if !idle {
            return false;
        }
        let Some(target_path) = self.freshest_hot_path() else {
            return false;
        };
        let current_path = self.selected_filtered_status_path();
        if current_path.as_deref() == Some(target_path.as_str()) {
            return false;
        }
        if self.git.view.auto_follow.followed_path.as_deref() == Some(target_path.as_str()) {
            return false;
        }
        let moved = self.select_status_file_by_path(&target_path);
        if moved {
            self.git.view.auto_follow.followed_path = Some(target_path);
        }
        moved
    }

    fn freshest_hot_path(&self) -> Option<String> {
        if self.git.view.status.hot_table.is_empty() {
            return None;
        }
        let now = SystemTime::now();
        let window = Duration::from_secs(self.git.agent_indicator.hot_window_secs);
        let mut best: Option<(&str, SystemTime)> = None;
        for &idx in self.filtered_indices() {
            let Some(file) = self.git.view.status.files.get(idx) else {
                continue;
            };
            let Some(&mtime) = self.git.view.status.hot_table.get(&file.path) else {
                continue;
            };
            // Future mtimes (clock skew on NFS, VMs) would pin auto-follow to
            // one bogus file forever; drop them — recovery is automatic once
            // the real clock catches up.
            let Ok(age) = now.duration_since(mtime) else {
                continue;
            };
            if age > window {
                continue;
            }
            let replace = match best {
                None => true,
                Some((bp, bm)) => mtime > bm || (mtime == bm && file.path.as_str() < bp),
            };
            if replace {
                best = Some((file.path.as_str(), mtime));
            }
        }
        best.map(|(p, _)| p.to_string())
    }

    fn select_status_file_by_path(&mut self, path: &str) -> bool {
        if let Some(idx) = self
            .git
            .view
            .status
            .files
            .iter()
            .position(|f| f.path == path)
            && self.git.view.status.selected != idx
        {
            self.git.view.status.selected = idx;
            self.git.view.status.file_scroll_x = 0;
            return true;
        }
        false
    }
}
