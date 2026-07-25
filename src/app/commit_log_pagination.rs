//! Background commit-log page fetcher pagination state.

use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use crate::platform::threading::{REAP_TIMEOUT, try_timed_join};

use super::commit_log_fetch::CommitLogPageMsg;

// Drop mirrors `SnapshotChannel`/`PtyPane`: dropping `page_rx` makes the
// worker's `tx.send` fail, then the JoinHandle is awaited so `change_repo`
// can't leak the old-repo worker.
#[derive(Default)]
pub struct CommitLogPagination {
    pub page_size: usize,
    pub prefetch_threshold: usize,
    pub(crate) page_rx: Option<Receiver<CommitLogPageMsg>>,
    // `cancel_commit_log_page_fetch` deliberately does NOT join here: the UI
    // tick can't wait for a mid-`load_commit_log_page` worker. Receiver-drop
    // already makes the reply fail; detaching is safe (worst case: one extra
    // OS thread until it finishes).
    pub(crate) handle: Option<JoinHandle<()>>,
    pub(crate) last_head_oid: Option<git2::Oid>,
}

impl CommitLogPagination {
    // `..Default::default()` can't be used: the type implements `Drop`.
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
        // Drop receiver first so the worker's next `tx.send` fails and the
        // loop exits; then bounded-join so a stuck libgit2 call can't freeze
        // shutdown / repo switch.
        drop(self.page_rx.take());
        if let Some(h) = self.handle.take() {
            try_timed_join(h, REAP_TIMEOUT);
        }
    }
}
