//! Background commit-log page fetcher.
//!
//! `git2::Repository` is `!Send`, so the worker thread opens its own handle
//! via `Repository::discover` against `App::repo_path`. The result returns
//! to the main thread through `mpsc::channel`; the main loop polls
//! [`App::poll_commit_log_page_fetch`] each tick and appends or discards
//! the page.

use std::sync::mpsc;
use std::thread;

use git2::Repository;

use super::App;
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
        let page_size = self.cfg_commit_log_page_size;
        let repo_path = self.repo_path.clone();
        let (tx, rx) = mpsc::channel();
        self.commit_log_page_rx = Some(rx);
        thread::spawn(move || {
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
    }

    /// Drain any commit-log page reply that has arrived since the last
    /// tick. Safe to call every loop iteration: returns without work if
    /// no fetch is pending.
    pub(crate) fn poll_commit_log_page_fetch(&mut self) {
        let Some(rx) = self.commit_log_page_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(msg) => {
                self.commit_log_page_rx = None;
                self.handle_commit_log_page_msg(msg);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.commit_log_page_rx = None;
                self.log_view.clear_pending();
            }
        }
    }

    /// Tear down any in-flight worker and clear the pending flag.
    /// Used when the underlying repo changes so a result built against
    /// the old repo never lands in the new view.
    pub(crate) fn cancel_commit_log_page_fetch(&mut self) {
        self.commit_log_page_rx = None;
        self.log_view.clear_pending();
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
            Ok(page) => self.log_view.append_page(page, msg.page_size),
            Err(e) => {
                tracing::warn!(error = %e, "commit log page fetch failed");
                self.log_view.clear_pending();
            }
        }
    }
}
