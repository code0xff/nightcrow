use super::{App, NoticeKind, RepoSnapshot, SnapshotMsg, ViewMode};
use std::collections::HashMap;
use std::time::SystemTime;

impl App {
    // Only the most recent message reflects current repo state, so a burst
    // collapses to one. Applying is NOT done here: this half touches no git
    // state, so every project can run it every tick to keep its unbounded
    // channel from growing, regardless of which tab is shown.
    pub fn drain_snapshot(&mut self) {
        while let Ok(msg) = self.snapshot.try_recv() {
            self.pending_snapshot = Some(msg);
        }
    }

    // Applying runs a full `refresh_diff`, so this is for the on-screen project
    // only — hidden projects' snapshots wait in `pending_snapshot` and apply on
    // the first tick after their tab comes forward.
    pub fn poll_snapshot(&mut self) {
        self.drain_snapshot();
        match self.pending_snapshot.take() {
            Some(SnapshotMsg::Ok(snapshot, mtimes)) => {
                self.ingest_snapshot(snapshot, mtimes);
            }
            Some(SnapshotMsg::Err(e)) => {
                tracing::warn!(error = %e, "git snapshot failed");
                self.raise_notice(NoticeKind::Git, e.to_string());
                // Pending restore is kept: the worker retries, and a later
                // snapshot should still apply the saved selection. Saving must
                // not be blocked by it — see `session_to_save`, which merges.
            }
            None => {}
        }
    }

    // Split out so tests can drive the merge/auto-follow logic with deterministic
    // mtimes instead of booting the background worker.
    pub fn ingest_snapshot(&mut self, snapshot: RepoSnapshot, mtimes: HashMap<String, SystemTime>) {
        // A selection waiting on this very snapshot stands in as the previous
        // path, so the ordinary "keep the cursor on the same file" machinery
        // performs the restore — no separate restore step to collide with.
        let previous_path = self
            .status_view
            .files
            .get(self.status_view.selected)
            .map(|f| f.path.clone())
            .or_else(|| {
                self.pending_selection
                    .as_ref()
                    .map(|(path, _)| path.clone())
            });
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
        self.clear_notice(NoticeKind::Git);

        // Skip on the very first snapshot (prior == None) so initial loads
        // don't double-fetch the commit log on top of `toggle_mode`'s eager load.
        let prior_head = self.pagination.last_head_oid;
        self.pagination.last_head_oid = new_head;
        if prior_head.is_some() && prior_head != new_head && self.mode == ViewMode::Log {
            self.refresh_commit_log_after_head_change();
        }

        // The saved scroll belongs to the saved file, so it only applies if
        // the cursor actually landed there.
        if let Some((path, scroll)) = self.pending_selection.take()
            && self.selected_filtered_status_path().as_deref() == Some(path.as_str())
        {
            self.diff.scroll = scroll.min(self.diff.max_scroll());
        }
    }

    // A path whose previous mtime was newer than the freshly observed one keeps
    // its previous mtime — a rename-from-stash can resurrect older mtimes for
    // the same path and must not demote a recent edit to cool. Updates in place
    // instead of rebuilding the HashMap every tick: the steady state has the
    // same path set tick after tick.
    pub(crate) fn merge_hot_table(&mut self, mtimes: HashMap<String, SystemTime>) {
        let table = &mut self.status_view.hot_table;
        table.retain(|path, _| mtimes.contains_key(path));
        for (path, new_mtime) in mtimes {
            table
                .entry(path)
                .and_modify(|stored| {
                    if new_mtime > *stored {
                        *stored = new_mtime;
                    }
                })
                .or_insert(new_mtime);
        }
    }
}