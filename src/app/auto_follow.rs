use super::{App, Focus, ViewMode};
use std::time::{Duration, Instant, SystemTime};

impl App {
    /// Mark that the user just navigated manually so auto-follow stays out
    /// for a short grace period. Also clears the "we steered to this path"
    /// memory — the user has taken back control.
    pub(crate) fn mark_user_navigated(&mut self) {
        self.auto_follow.last_manual_nav_at = Some(Instant::now());
        self.auto_follow.followed_path = None;
    }

    /// Decide whether the file list should auto-follow to a new hot file,
    /// and perform the move if so. Returns `true` when selection changed.
    /// Caller is responsible for refreshing the diff afterward.
    pub(crate) fn try_auto_follow(&mut self) -> bool {
        if !self.cfg_agent_indicator.enabled || !self.cfg_agent_indicator.auto_follow {
            return false;
        }
        if self.focus != Focus::FileList || self.mode != ViewMode::Status {
            return false;
        }
        let idle = match self.auto_follow.last_manual_nav_at {
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
        if self.auto_follow.followed_path.as_deref() == Some(target_path.as_str()) {
            return false;
        }
        let moved = self.select_status_file_by_path(&target_path);
        if moved {
            self.auto_follow.followed_path = Some(target_path);
        }
        moved
    }

    /// Path with the newest mtime among files that are still inside the
    /// configured hot window and pass the current filter. Returns `None`
    /// when no qualifying file exists. Tiebreak by path for stability.
    fn freshest_hot_path(&self) -> Option<String> {
        if self.status_view.hot_table.is_empty() {
            return None;
        }
        let now = SystemTime::now();
        let window = Duration::from_secs(self.cfg_agent_indicator.hot_window_secs);
        // Walk the filtered index list and probe `hot_table` by path. The
        // previous implementation built a per-tick `HashSet` of filtered
        // paths inside `try_auto_follow`'s hot loop, which allocated every
        // snapshot tick. Tiebreak by smaller path for stability.
        let mut best: Option<(&str, SystemTime)> = None;
        for &idx in self.filtered_indices() {
            let Some(file) = self.status_view.files.get(idx) else {
                continue;
            };
            let Some(&mtime) = self.status_view.hot_table.get(&file.path) else {
                continue;
            };
            // `duration_since` returns Err when `mtime > now` (clock skew on
            // NFS, VMs, files touched with a future stamp). Treating those as
            // permanently in-window (`unwrap_or(true)`) would pin auto-follow
            // to a single bogus file forever — no later edit can ever beat a
            // future timestamp. Floor the timestamp at `now` instead so a
            // future-stamped file is treated as "just edited."
            let effective = mtime.min(now);
            let in_window = now
                .duration_since(effective)
                .map(|d| d <= window)
                .unwrap_or(false);
            if !in_window {
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

    /// Move the selection cursor to `path` if it exists in the unfiltered
    /// status list. Returns whether selection actually changed.
    fn select_status_file_by_path(&mut self, path: &str) -> bool {
        if let Some(idx) = self.status_view.files.iter().position(|f| f.path == path)
            && self.status_view.selected != idx
        {
            self.status_view.selected = idx;
            self.status_view.file_scroll_x = 0;
            return true;
        }
        false
    }
}
