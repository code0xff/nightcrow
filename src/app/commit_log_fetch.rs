//! Background commit-log page fetcher.
//!
//! `git2::Repository` is `!Send`, so the worker thread opens its own handle
//! via `Repository::discover` against `App::repo_path`. The result returns
//! to the main thread through `mpsc::channel`; the main loop polls
//! [`App::poll_commit_log_page_fetch`] each tick and appends or discards
//! the page.

use std::sync::mpsc;
use std::thread;

use crate::util::{REAP_TIMEOUT, try_timed_join};

use git2::{Oid, Repository};

use super::App;
use super::ViewMode;
use crate::git::diff::{CommitEntry, load_commit_log_page};

/// Distinguishes the two ways a worker reply must be merged into the view.
///
/// `Tail` is the prefetch case: extend the loaded list at the current tail.
/// `Refresh` is the head-anchor case: replace or prepend onto the existing
/// list, using the snapshot of selection / head oid captured at spawn time
/// so the post-load merge stays deterministic even if the user navigated
/// while the worker was running.
pub(crate) enum CommitLogFetchKind {
    Tail,
    Refresh {
        prior_selected_oid: Option<Oid>,
        prior_head_oid: Option<Oid>,
    },
}

/// One reply from a paged fetch worker.
///
/// `skip` is the offset the worker was launched with: the main thread
/// uses it as a stale-result check before appending — if the loaded
/// commit count has changed between spawn and reply (HEAD refresh,
/// repo switch), the page is dropped.
pub(crate) struct CommitLogPageMsg {
    pub kind: CommitLogFetchKind,
    pub skip: usize,
    pub page_size: usize,
    pub result: Result<Vec<CommitEntry>, String>,
}

impl App {
    /// Spawn a background worker that fetches the next page of the
    /// commit log starting at `skip`. Returns immediately. If a fetch
    /// is already pending or the log is fully loaded, this is a no-op.
    pub(crate) fn spawn_commit_log_page_fetch(&mut self, skip: usize) {
        if self.log_view.fully_loaded {
            return;
        }
        if !self.log_view.mark_pending() {
            return;
        }
        self.launch_commit_log_worker(skip, CommitLogFetchKind::Tail);
    }

    /// Spawn a worker that fetches page 0 to refresh the cached commit
    /// list, capturing the prior selection/head oids so the merge at
    /// reply time can preserve the user's view. Used both for initial
    /// Log-mode entry (prior_*_oid = None) and for HEAD-change refresh.
    pub(crate) fn spawn_commit_log_refresh_fetch(
        &mut self,
        prior_selected_oid: Option<Oid>,
        prior_head_oid: Option<Oid>,
    ) {
        if !self.log_view.mark_pending() {
            return;
        }
        self.launch_commit_log_worker(
            0,
            CommitLogFetchKind::Refresh {
                prior_selected_oid,
                prior_head_oid,
            },
        );
    }

    /// Shared spawn helper. Detaches any previous handle (no join on the
    /// UI thread — the receiver-drop already signals the worker to exit
    /// at next send, and an old handle that's mid-`load_commit_log_page`
    /// must not stall the frame). Installs the new receiver+handle.
    fn launch_commit_log_worker(&mut self, skip: usize, kind: CommitLogFetchKind) {
        drop(self.pagination.page_rx.take());
        // Detach prior handle: the worker keeps running until it tries
        // to send, then exits cleanly.
        self.pagination.handle.take();
        let page_size = self.pagination.page_size;
        let repo_path = self.repo_path.clone();
        let (tx, rx) = mpsc::channel();
        self.pagination.page_rx = Some(rx);
        let handle = thread::spawn(move || {
            let result = match Repository::discover(&repo_path) {
                Ok(repo) => load_commit_log_page(&repo, skip, page_size).map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = tx.send(CommitLogPageMsg {
                kind,
                skip,
                page_size,
                result,
            });
        });
        self.pagination.handle = Some(handle);
    }

    /// Drain any commit-log page reply that has arrived since the last
    /// tick. Safe to call every loop iteration: returns without work if
    /// no fetch is pending.
    pub(crate) fn poll_commit_log_page_fetch(&mut self) {
        let Some(rx) = self.pagination.page_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(msg) => {
                self.pagination.page_rx = None;
                // Worker just sent → it is one statement away from
                // returning. A short timed join reaps the OS thread now
                // instead of waiting for `Drop`, and the timeout means a
                // wedged worker still can't stall the frame.
                if let Some(h) = self.pagination.handle.take() {
                    try_timed_join(h, REAP_TIMEOUT);
                }
                self.handle_commit_log_page_msg(msg);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pagination.page_rx = None;
                if let Some(h) = self.pagination.handle.take() {
                    try_timed_join(h, REAP_TIMEOUT);
                }
                self.log_view.clear_pending();
            }
        }
    }

    /// Tear down any in-flight worker and clear the pending flag.
    /// Called on repo switch (a discrete user action, not a per-tick
    /// hot path), so a short timed join is affordable here. The
    /// receiver-drop above flips the worker's next `tx.send` to Err and
    /// the join completes in microseconds in the common case; the
    /// timeout caps worst-case latency if a worker is wedged.
    pub(crate) fn cancel_commit_log_page_fetch(&mut self) {
        drop(self.pagination.page_rx.take());
        if let Some(h) = self.pagination.handle.take() {
            try_timed_join(h, REAP_TIMEOUT);
        }
        self.log_view.clear_pending();
    }

    /// If the current Log view selection is within
    /// `pagination.prefetch_threshold` rows of the loaded tail, start a
    /// background page fetch from `loaded_count`. No-ops in Status mode,
    /// drill-down, empty list, pending, and fully-loaded states.
    pub(crate) fn maybe_prefetch_commit_log(&mut self) {
        if self.mode != ViewMode::Log {
            return;
        }
        if self.log_view.drill_down {
            return;
        }
        // Pause tail prefetch while the commit-list search bar is open.
        // A new page arriving mid-search would shift the filter cache
        // and disturb the user's view; the gate is lifted by
        // `cancel_log_search` / `confirm_log_search`, which re-call this
        // helper on the way out.
        if self.log_view.commit_search_active {
            return;
        }
        if self.log_view.commits.is_empty() {
            return;
        }
        if self.log_view.pending_fetch || self.log_view.fully_loaded {
            return;
        }
        let loaded = self.log_view.loaded_count;
        let threshold = self.pagination.prefetch_threshold;
        // Trigger when the user is close enough to the tail that the
        // next handful of moves would scroll past the loaded range.
        if self.log_view.selected + threshold >= loaded {
            self.spawn_commit_log_page_fetch(loaded);
        }
    }

    /// Block until any pending commit-log fetch has been drained and
    /// applied. Test-only — production code polls each tick via the
    /// main loop and never needs to wait.
    #[cfg(test)]
    pub(crate) fn flush_commit_log_fetch_for_test(&mut self, timeout: std::time::Duration) {
        let start = std::time::Instant::now();
        while self.log_view.pending_fetch {
            if start.elapsed() > timeout {
                panic!("commit log fetch did not complete within {:?}", timeout);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            self.poll_commit_log_page_fetch();
        }
    }
}