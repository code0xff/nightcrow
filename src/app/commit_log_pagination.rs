//! Background commit-log page fetcher pagination state.
//!
//! Lifted off `App` so the page worker's lifecycle (receiver + JoinHandle)
//! lives in one place and the related config knobs and HEAD anchor don't
//! sprawl across `App`'s top-level fields.

use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use crate::util::{REAP_TIMEOUT, try_timed_join};

use super::commit_log_fetch::CommitLogPageMsg;

/// Owns the commit-log pagination state. The Drop impl mirrors
/// `SnapshotChannel` / `PtyPane`: dropping `page_rx` makes the worker's
/// `tx.send` fail at the next reply, then the JoinHandle is awaited so a
/// `change_repo` cannot leak the old-repo worker into the new view.
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
    /// before `Pagination` itself goes away. `cancel_commit_log_page_fetch`
    /// deliberately does *not* join here: the UI tick can't afford to wait
    /// for a worker that's mid-`load_commit_log_page`. The receiver-drop
    /// already makes the worker's reply fail, so detaching the handle is
    /// safe — the worst case is one extra OS thread until it finishes.
    pub(crate) handle: Option<JoinHandle<()>>,
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
        // and the loop exits; then await the thread with a bounded
        // timeout — a worker stuck mid-`load_commit_log_page` on libgit2
        // must not freeze app shutdown / repo switch.
        drop(self.page_rx.take());
        if let Some(h) = self.handle.take() {
            try_timed_join(h, REAP_TIMEOUT);
        }
    }
}