//! Background commit-log page fetcher pagination state.

use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;

use crate::platform::threading::{REAP_TIMEOUT, try_timed_join};

use super::commit_log_fetch::CommitLogPageMsg;

// Drop mirrors `SnapshotChannel`/`PtyPane`: dropping `page_rx` makes the
// worker's `tx.send` fail, then the JoinHandle is awaited so `change_repo`
// can't leak the old-repo worker.
#[derive(Default)]
pub struct CommitLogController {
    page_size: usize,
    prefetch_threshold: usize,
    pub(crate) page_rx: Option<Receiver<CommitLogPageMsg>>,
    // `cancel_commit_log_page_fetch` deliberately does NOT join here: the UI
    // tick can't wait for a mid-`load_commit_log_page` worker. Receiver-drop
    // already makes the reply fail; detaching is safe (worst case: one extra
    // OS thread until it finishes).
    pub(crate) handle: Option<JoinHandle<()>>,
    last_head_oid: Option<git2::Oid>,
    generation: u64,
}

impl CommitLogController {
    // `..Default::default()` can't be used: the type implements `Drop`.
    pub fn with_config(page_size: usize, prefetch_threshold: usize) -> Self {
        Self {
            page_size,
            prefetch_threshold,
            page_rx: None,
            handle: None,
            last_head_oid: None,
            generation: 0,
        }
    }

    pub fn configure(&mut self, page_size: usize, prefetch_threshold: usize) {
        self.page_size = page_size;
        self.prefetch_threshold = prefetch_threshold;
    }

    pub(crate) fn page_size(&self) -> usize {
        self.page_size
    }
    pub(crate) fn prefetch_threshold(&self) -> usize {
        self.prefetch_threshold
    }
    pub(crate) fn last_head_oid(&self) -> Option<git2::Oid> {
        self.last_head_oid
    }
    pub(crate) fn set_last_head_oid(&mut self, oid: Option<git2::Oid>) {
        self.last_head_oid = oid;
    }
    pub(crate) fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.generation
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    #[cfg(test)]
    pub(crate) fn fetch_pending(&self) -> bool {
        self.page_rx.is_some()
    }
}

impl Drop for CommitLogController {
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

#[cfg(test)]
mod tests {
    use super::CommitLogController;

    #[test]
    fn generation_advances_for_each_request_and_cancel() {
        let mut controller = CommitLogController::with_config(50, 10);

        let first = controller.next_generation();
        let second = controller.next_generation();

        assert_ne!(first, second);
        assert_eq!(controller.generation(), second);
    }

    #[test]
    fn configuration_is_exposed_without_worker_internals() {
        let mut controller = CommitLogController::with_config(50, 10);
        controller.configure(25, 5);

        assert_eq!(controller.page_size(), 25);
        assert_eq!(controller.prefetch_threshold(), 5);
        assert!(!controller.fetch_pending());
    }
}
