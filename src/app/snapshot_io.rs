use super::{App, RepoSnapshot, SnapshotMsg, ViewMode};
use std::collections::HashMap;
use std::time::SystemTime;

impl App {
    pub fn poll_snapshot(&mut self) {
        while let Ok(msg) = self.snapshot.try_recv() {
            match msg {
                SnapshotMsg::Ok(snapshot, mtimes) => {
                    self.ingest_snapshot(snapshot, mtimes);
                }
                SnapshotMsg::Err(e) => {
                    tracing::warn!(error = %e, "git snapshot failed");
                    self.status = Some(format!("git error: {e}"));
                }
            }
        }
    }

    /// Apply a snapshot to app state. Split out from `poll_snapshot` so
    /// tests can drive the merge/auto-follow logic with deterministic
    /// mtimes instead of booting the background worker.
    pub fn ingest_snapshot(&mut self, snapshot: RepoSnapshot, mtimes: HashMap<String, SystemTime>) {
        let previous_path = self
            .status_view
            .files
            .get(self.status_view.selected)
            .map(|f| f.path.clone());
        // Capture the HEAD oid up front so the detection branch below stays
        // independent of where the snapshot fields get moved out.
        let new_head = snapshot.head_oid;
        self.branch_name = snapshot.branch_name;
        self.status_view.set_files(snapshot.files);
        self.status_view.recompute_filter();
        self.tracking = snapshot.tracking;
        self.merge_hot_table(mtimes);

        self.restore_selection(previous_path.as_deref());
        self.sync_selection_to_filter();
        let auto_followed = self.try_auto_follow();
        let selected_path = self.selected_filtered_status_path();
        let selected_path_changed = auto_followed || selected_path != previous_path;
        if self.mode == ViewMode::Status {
            if selected_path.is_some() {
                self.refresh_diff(selected_path_changed);
            } else {
                self.clear_diff_state();
            }
        }
        if self
            .status
            .as_deref()
            .is_some_and(|msg| msg.starts_with("git error:"))
        {
            self.status = None;
        }

        // Detect commits made through the terminal pane or external tools and
        // refresh the Log view's cached commit list. Skip on the very first
        // snapshot (prior == None) so initial loads don't double-fetch the
        // commit log on top of `toggle_mode`'s eager load.
        let prior_head = self.last_head_oid;
        self.last_head_oid = new_head;
        if prior_head.is_some()
            && prior_head != new_head
            && self.mode == ViewMode::Log
        {
            self.refresh_commit_log_after_head_change();
        }

        if let Some(state) = self.pending_session.take() {
            self.restore_session(&state);
        }
    }

    /// Update `hot_table` with the latest observed mtimes. Entries for
    /// paths missing from the new snapshot are dropped; entries with a
    /// strictly newer mtime are replaced (so a file edited twice within
    /// the hot window re-arms its fade). A path whose previous mtime was
    /// newer than the freshly observed one keeps its previous mtime — a
    /// rename-from-stash can resurrect older mtimes for the same path
    /// and must not demote a recent edit to cool.
    pub(crate) fn merge_hot_table(&mut self, mtimes: HashMap<String, SystemTime>) {
        let prior = std::mem::take(&mut self.status_view.hot_table);
        let mut next = HashMap::with_capacity(mtimes.len());
        for (path, new_mtime) in mtimes {
            let final_mtime = match prior.get(&path) {
                Some(&stored) if stored > new_mtime => stored,
                _ => new_mtime,
            };
            next.insert(path, final_mtime);
        }
        self.status_view.hot_table = next;
    }
}
