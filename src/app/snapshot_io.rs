use super::{App, NoticeKind, RepoSnapshot, SnapshotMsg, ViewMode};
use std::collections::HashMap;
use std::time::SystemTime;

impl App {
    /// Empty the worker's queue, keeping only the tail in `pending_snapshot`.
    ///
    /// Only the most recent message reflects current repo state, so a burst
    /// (e.g. while the main loop was blocked by a synchronous git2 call)
    /// collapses to one. Applying it is deliberately NOT done here: this half
    /// touches no git state, so every open project can run it every tick to
    /// keep its unbounded channel from growing, no matter which tab is shown.
    pub fn drain_snapshot(&mut self) {
        while let Ok(msg) = self.snapshot.try_recv() {
            self.pending_snapshot = Some(msg);
        }
    }

    /// Drain, then apply whatever is pending.
    ///
    /// Applying runs a full `refresh_diff`, so this is for the project on
    /// screen only — doing it for hidden projects would put several
    /// repositories' git diffs on the UI thread every tick. A background
    /// project's snapshot waits in `pending_snapshot` and is applied on the
    /// first tick after its tab comes forward.
    pub fn poll_snapshot(&mut self) {
        self.drain_snapshot();
        match self.pending_snapshot.take() {
            Some(SnapshotMsg::Ok(snapshot, mtimes)) => {
                self.ingest_snapshot(snapshot, mtimes);
            }
            Some(SnapshotMsg::Err(e)) => {
                tracing::warn!(error = %e, "git snapshot failed");
                self.raise_notice(NoticeKind::Git, e.to_string());
                // Give up on the deferred half of a session restore. Panes and
                // focus were already restored at startup; only selection and
                // scroll wait on a file list, and this project may never get
                // one. Left pending, the project would be skipped by every
                // save from here on (see `save_project_session`) and lose the
                // state it does have.
                self.pending_session = None;
            }
            None => {}
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
        self.clear_notice(NoticeKind::Git);

        // Detect commits made through the terminal pane or external tools and
        // refresh the Log view's cached commit list. Skip on the very first
        // snapshot (prior == None) so initial loads don't double-fetch the
        // commit log on top of `toggle_mode`'s eager load.
        let prior_head = self.pagination.last_head_oid;
        self.pagination.last_head_oid = new_head;
        if prior_head.is_some() && prior_head != new_head && self.mode == ViewMode::Log {
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
    ///
    /// Updates the existing map in place instead of building a fresh
    /// HashMap on every snapshot tick: the typical steady state has the
    /// same path set tick after tick, so the prior strategy churned the
    /// allocator on the UI poll path for no behavioural benefit.
    pub(crate) fn merge_hot_table(&mut self, mtimes: HashMap<String, SystemTime>) {
        let table = &mut self.status_view.hot_table;
        // Drop entries that are no longer in the snapshot.
        table.retain(|path, _| mtimes.contains_key(path));
        // Upsert: insert new paths, keep the newer mtime when both exist.
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
