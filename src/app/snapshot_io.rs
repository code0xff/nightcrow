use super::{App, NoticeKind, RepoSnapshot, SnapshotMsg, ViewMode};
use std::collections::HashMap;
use std::time::SystemTime;

impl App {
    // Only the most recent message reflects current repo state, so a burst
    // collapses to one. Applying is NOT done here: this half touches no git
    // state, so every project can run it every tick to keep its unbounded
    // channel from growing, regardless of which tab is shown.
    pub fn drain_snapshot(&mut self) -> bool {
        let mut received = false;
        while let Ok(msg) = self.git.snapshot.try_recv() {
            received = true;
            self.git.pending_snapshot = Some(msg);
        }
        received
    }

    // Applying runs a full `refresh_diff`, so this is for the on-screen project
    // only — hidden projects' snapshots wait in `pending_snapshot` and apply on
    // the first tick after their tab comes forward.
    pub fn poll_snapshot(&mut self) -> bool {
        let received = self.drain_snapshot();
        match self.git.pending_snapshot.take() {
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
            None => return received,
        }
        true
    }

    // Split out so tests can drive the merge/auto-follow logic with deterministic
    // mtimes instead of booting the background worker.
    pub fn ingest_snapshot(&mut self, snapshot: RepoSnapshot, mtimes: HashMap<String, SystemTime>) {
        // A selection waiting on this very snapshot stands in as the previous
        // path, so the ordinary "keep the cursor on the same file" machinery
        // performs the restore — no separate restore step to collide with.
        let previous_path = self
            .git
            .view
            .status
            .files
            .get(self.git.view.status.selected)
            .map(|f| f.path.clone())
            .or_else(|| {
                self.git
                    .view
                    .pending_selection()
                    .map(|(path, _)| path.clone())
            });
        let previous_selected = previous_path.as_ref().and_then(|path| {
            self.git
                .view
                .status
                .files
                .iter()
                .find(|file| &file.path == path)
                .cloned()
        });
        let previous_snapshot_mtime = self.git.view.selected_snapshot_mtime.clone().or_else(|| {
            previous_path.as_ref().map(|path| {
                (
                    path.clone(),
                    self.git.view.status.hot_table.get(path).copied(),
                )
            })
        });
        let new_head = snapshot.head_oid;
        self.git.branch_name = snapshot.branch_name;
        self.refresh_log_decorations(snapshot.refs_fingerprint);
        self.git.view.status.set_files(snapshot.files);
        self.git.view.status_mut().recompute_filter();
        self.git.tracking = snapshot.tracking;
        self.merge_hot_table(&mtimes);

        self.restore_selection(previous_path.as_deref());
        self.sync_selection_to_filter();
        let auto_followed = self.try_auto_follow();
        let selected_path = self.selected_filtered_status_path();
        let selected_snapshot_mtime = selected_path
            .as_ref()
            .map(|path| (path.clone(), mtimes.get(path).copied()));
        let selected_path_changed = auto_followed || selected_path != previous_path;
        let selected_state_unchanged = !selected_path_changed
            && previous_selected.as_ref().is_some_and(|previous| {
                self.selected_filtered_status_file().is_some_and(|current| {
                    current.path == previous.path
                        && current.old_path == previous.old_path
                        && current.index == previous.index
                        && current.worktree == previous.worktree
                        && previous_snapshot_mtime == selected_snapshot_mtime
                })
            });
        self.git.view.selected_snapshot_mtime = selected_snapshot_mtime;
        if self.git.view.mode == ViewMode::Status {
            if selected_path.is_some() {
                if !selected_state_unchanged {
                    self.refresh_diff(selected_path_changed);
                }
            } else {
                self.clear_diff_state();
            }
        }
        self.clear_notice(NoticeKind::Git);

        // Skip on the very first snapshot (prior == None) so initial loads
        // don't double-fetch the commit log on top of `toggle_mode`'s eager load.
        let prior_head = self.git.commit_log.last_head_oid();
        self.git.commit_log.set_last_head_oid(new_head);
        if prior_head.is_some() && prior_head != new_head && self.git.view.mode == ViewMode::Log {
            self.refresh_commit_log_after_head_change();
        }

        // The saved scroll belongs to the saved file, so it only applies if
        // the cursor actually landed there.
        if let Some((path, scroll)) = self.git.view.take_pending_selection()
            && self.selected_filtered_status_path().as_deref() == Some(path.as_str())
        {
            self.git.view.diff.scroll = scroll.min(self.git.view.diff.max_scroll());
        }
    }

    // Rebuilding walks every ref and peels each one, so it is gated on the
    // fingerprint rather than run per poll. A failure leaves the previous map in
    // place: stale chips beat chips vanishing on a transient read error.
    fn refresh_log_decorations(&mut self, fingerprint: u64) {
        if self.git.last_refs_fingerprint == Some(fingerprint) {
            return;
        }
        self.git
            .load_controller
            .request_decorations(&self.git.repo_path, fingerprint);
    }

    // A path whose previous mtime was newer than the freshly observed one keeps
    // its previous mtime — a rename-from-stash can resurrect older mtimes for
    // the same path and must not demote a recent edit to cool. Updates in place
    // instead of rebuilding the HashMap every tick: the steady state has the
    // same path set tick after tick.
    pub(crate) fn merge_hot_table(&mut self, mtimes: &HashMap<String, SystemTime>) {
        let table = &mut self.git.view.status.hot_table;
        table.retain(|path, _| mtimes.contains_key(path));
        for (path, new_mtime) in mtimes {
            if let Some(stored) = table.get_mut(path) {
                if new_mtime > stored {
                    *stored = *new_mtime;
                }
            } else {
                table.insert(path.clone(), *new_mtime);
            }
        }
    }
}
