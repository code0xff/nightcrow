//! Background commit-log page fetcher.
//!
//! `git2::Repository` is `!Send`, so the worker thread opens its own handle
//! via `Repository::discover` against `App::repo_path`. The result returns
//! to the main thread through `mpsc::channel`; the main loop polls
//! [`App::poll_commit_log_page_fetch`] each tick and appends or discards
//! the page.

use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use git2::Repository;

use super::App;
use super::ViewMode;
use crate::git::diff::{CommitEntry, load_commit_log_page};

/// One reply from a paged fetch worker.
///
/// `skip` is the offset the worker was launched with: the main thread
/// uses it as a stale-result check before appending — if the loaded
/// commit count has changed between spawn and reply (HEAD refresh,
/// repo switch), the page is dropped.
pub(crate) struct CommitLogPageMsg {
    pub skip: usize,
    pub page_size: usize,
    pub result: Result<Vec<CommitEntry>, String>,
}

/// Owns the commit-log pagination state. Lifted off `App` so the page
/// worker's lifecycle (receiver + JoinHandle) lives in one place and so
/// the related config knobs and HEAD anchor don't sprawl across `App`'s
/// top-level fields. The Drop impl mirrors `SnapshotChannel` / `PtyPane`:
/// dropping `page_rx` makes the worker's `tx.send` fail at the next reply,
/// then the JoinHandle is awaited so a `change_repo` cannot leak the
/// old-repo worker into the new view.
#[derive(Default)]
pub struct CommitLogPagination {
    /// Commits loaded per page. Sourced from `LogConfig::commit_log_page_size`.
    pub page_size: usize,
    /// Prefetch begins when the cursor is within this many rows of the
    /// loaded tail. Sourced from `LogConfig::commit_log_prefetch_threshold`.
    pub prefetch_threshold: usize,
    /// Receiver for the in-flight worker. `Some` while a fetch is pending;
    /// cleared once drained or cancelled.
    pub(crate) page_rx: Option<Receiver<CommitLogPageMsg>>,
    /// JoinHandle for the in-flight worker, awaited on `Drop` so the
    /// channel-close → tx.send-error → thread-exit sequence completes
    /// before `Pagination` itself goes away.
    handle: Option<JoinHandle<()>>,
    /// HEAD oid carried in the most recent snapshot. `ingest_snapshot`
    /// compares this against the new snapshot's head to decide whether
    /// to trigger `refresh_commit_log_after_head_change`.
    pub(crate) last_head_oid: Option<git2::Oid>,
}

impl CommitLogPagination {
    /// Construct with the config-derived knobs and otherwise default state.
    /// Used by `App::new` and the test fixture — `..Default::default()`
    /// can't be used here because the type implements `Drop`.
    pub fn with_config(page_size: usize, prefetch_threshold: usize) -> Self {
        Self {
            page_size,
            prefetch_threshold,
            page_rx: None,
            handle: None,
            last_head_oid: None,
        }
    }
}

impl Drop for CommitLogPagination {
    fn drop(&mut self) {
        // Drop the receiver first so the worker's next `tx.send` fails
        // and the loop exits; then await the thread.
        drop(self.page_rx.take());
        if let Some(h) = self.handle.take()
            && let Err(e) = h.join()
        {
            tracing::warn!(?e, "commit-log page worker panicked during shutdown");
        }
    }
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
        // Drop any stale receiver+worker before installing the new one.
        // `mark_pending` guards against a concurrent spawn for the same
        // tail, but a leftover handle from a previously-cleared
        // `pending_fetch` (e.g. via `clear_pending` on a worker error)
        // would otherwise be silently overwritten.
        drop(self.pagination.page_rx.take());
        if let Some(h) = self.pagination.handle.take() {
            let _ = h.join();
        }
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
                // The worker that produced this message has already finished
                // its tx.send; join is non-blocking. Skip a stale-handle log
                // entry by ignoring the result.
                if let Some(h) = self.pagination.handle.take() {
                    let _ = h.join();
                }
                self.handle_commit_log_page_msg(msg);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pagination.page_rx = None;
                if let Some(h) = self.pagination.handle.take() {
                    let _ = h.join();
                }
                self.log_view.clear_pending();
            }
        }
    }

    /// Tear down any in-flight worker and clear the pending flag.
    /// Used when the underlying repo changes so a result built against
    /// the old repo never lands in the new view.
    pub(crate) fn cancel_commit_log_page_fetch(&mut self) {
        // Drop the receiver so the worker's next tx.send fails, then
        // join: matches the SnapshotChannel/PtyPane discipline so a
        // change_repo cannot leak an old-repo worker.
        drop(self.pagination.page_rx.take());
        if let Some(h) = self.pagination.handle.take() {
            let _ = h.join();
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

    fn handle_commit_log_page_msg(&mut self, msg: CommitLogPageMsg) {
        // Stale-result check: the worker was launched with `skip` equal
        // to the loaded count at the time. If the count has changed
        // (HEAD refresh resetting pagination, repo switch landing
        // before this reply, etc.), the page no longer concatenates
        // safely onto the current list.
        if msg.skip != self.log_view.loaded_count {
            self.log_view.clear_pending();
            return;
        }
        match msg.result {
            Ok(page) => {
                self.log_view.append_page(page, msg.page_size);
                // Chain another fetch immediately if the user is still
                // sitting near the new tail; otherwise the next
                // selection move would have to wait a tick.
                self.maybe_prefetch_commit_log();
            }
            Err(e) => {
                tracing::warn!(error = %e, "commit log page fetch failed");
                self.log_view.clear_pending();
            }
        }
    }
}
