//! Background commit-log page fetcher.
//!
//! `git2::Repository` is `!Send`, so the worker thread opens its own handle
//! via `Repository::discover` against `App::repo_path`. The result returns
//! to the main thread through `mpsc::channel`; the main loop polls
//! [`App::poll_commit_log_page_fetch`] each tick and appends or discards
//! the page.

use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

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
    /// before `Pagination` itself goes away. `cancel_commit_log_page_fetch`
    /// deliberately does *not* join here: the UI tick can't afford to wait
    /// for a worker that's mid-`load_commit_log_page`. The receiver-drop
    /// already makes the worker's reply fail, so detaching the handle is
    /// safe — the worst case is one extra OS thread until it finishes.
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
                // Detach the worker handle: it has finished sending and
                // will exit cleanly. Joining here would be free but
                // pointless — the OS reaps it either way.
                self.pagination.handle.take();
                self.handle_commit_log_page_msg(msg);
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pagination.page_rx = None;
                self.pagination.handle.take();
                self.log_view.clear_pending();
            }
        }
    }

    /// Tear down any in-flight worker and clear the pending flag.
    /// Used when the underlying repo changes so a result built against
    /// the old repo never lands in the new view. The handle is detached
    /// rather than joined — see `launch_commit_log_worker` for why.
    pub(crate) fn cancel_commit_log_page_fetch(&mut self) {
        drop(self.pagination.page_rx.take());
        self.pagination.handle.take();
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
        match msg.kind {
            CommitLogFetchKind::Tail => self.apply_tail_page(msg),
            CommitLogFetchKind::Refresh {
                prior_selected_oid,
                prior_head_oid,
            } => self.apply_refresh_page(msg, prior_selected_oid, prior_head_oid),
        }
    }

    fn apply_tail_page(&mut self, msg: CommitLogPageMsg) {
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

    /// Apply a fresh page-0 fetch as a refresh: either prepend new head
    /// commits onto the cached tail (fast-forward), or replace the list
    /// outright (divergence, initial entry). Mirrors the merge that was
    /// previously inline in `refresh_commit_log_after_head_change`, now
    /// driven off a captured snapshot of the pre-spawn state so the
    /// load itself can run on a worker thread.
    fn apply_refresh_page(
        &mut self,
        msg: CommitLogPageMsg,
        prior_selected_oid: Option<Oid>,
        prior_head_oid: Option<Oid>,
    ) {
        let page_size = msg.page_size;
        let page = match msg.result {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(error = %e, "commit log refresh fetch failed");
                self.log_view.clear_pending();
                return;
            }
        };

        // If the previous head still appears in the freshly fetched first
        // page and the fresh tail lines up with the cached list, treat the
        // change as a fast-forward / simple new commit: prepend the newer
        // entries onto the existing list so all accumulated pages stay valid.
        // A merge can interleave side-branch commits after the old head; in
        // that case cached pages are no longer a contiguous prefix of the
        // new revwalk, so reset to the freshly loaded first page instead.
        let prepend_idx = prior_head_oid.and_then(|oid| page.iter().position(|c| c.oid == oid));
        let page_is_short = page.len() < page_size;
        let can_prepend = prepend_idx.is_some_and(|idx| {
            let fresh_tail = &page[idx..];
            !self.log_view.commits.is_empty()
                && fresh_tail.len() <= self.log_view.commits.len()
                && fresh_tail
                    .iter()
                    .zip(self.log_view.commits.iter())
                    .all(|(fresh, cached)| fresh.oid == cached.oid)
        });
        if let Some(idx) = prepend_idx
            && can_prepend
        {
            let mut new_head_commits: Vec<_> = page.into_iter().take(idx).collect();
            let n_new = new_head_commits.len();
            new_head_commits.append(&mut self.log_view.commits);
            self.log_view.commits = new_head_commits;
            self.log_view.loaded_count = self.log_view.commits.len();
            // `page_is_short` only describes the freshly fetched first page;
            // it doesn't account for cached later pages. Preserve prior
            // completion state and only promote to fully_loaded when the
            // new revwalk demonstrably fits within one page.
            if page_is_short && self.log_view.commits.len() <= page_size {
                self.log_view.fully_loaded = true;
            }
            self.log_view.commit_width_cache.set(None);
            self.log_view.clear_pending();
            // Slide the selection so the user keeps looking at the same
            // commit even though new entries appeared above it.
            if let Some(prior_oid) = prior_selected_oid
                && let Some(pos) = self
                    .log_view
                    .commits
                    .iter()
                    .position(|c| c.oid == prior_oid)
            {
                self.log_view.selected = pos;
            } else {
                // `prior_selected_oid` was Some, so the cached list contained
                // that oid. If the position lookup fails despite the list
                // being a prefix of the new one — corruption, or a race we
                // haven't accounted for — clamp to the new bounds so a
                // downstream `commits.get(selected)` lands on the tail
                // instead of returning None and clearing the diff pane.
                self.log_view.selected = self
                    .log_view
                    .selected
                    .saturating_add(n_new)
                    .min(self.log_view.commits.len().saturating_sub(1));
            }
        } else {
            self.log_view.set_commits_from_first_page(page, page_size);
            self.log_view.selected = prior_selected_oid
                .and_then(|oid| self.log_view.commits.iter().position(|c| c.oid == oid))
                .unwrap_or(0);
        }
        self.log_view.commit_scroll_x = 0;
        // Anchor the head-oid sentinel to whatever we just loaded so
        // ingest_snapshot doesn't immediately trigger another refresh.
        self.pagination.last_head_oid = self.log_view.commits.first().map(|c| c.oid);

        // Drill-down survives only if the commit it was opened on is still
        // in the (possibly extended) list. Otherwise drop back to the
        // commit-level diff.
        if self.log_view.drill_down
            && prior_selected_oid
                .is_none_or(|oid| !self.log_view.commits.iter().any(|c| c.oid == oid))
        {
            self.log_view.reset_drill_down();
        }

        if self.log_view.drill_down {
            self.load_file_diff_for_log_file_selected();
        } else {
            self.load_commit_diff_for_selected();
        }

        self.maybe_prefetch_commit_log();
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
