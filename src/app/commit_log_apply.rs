use git2::Oid;

use super::App;
use super::commit_log_fetch::{CommitLogFetchKind, CommitLogPageMsg};

impl App {
    pub(super) fn handle_commit_log_page_msg(&mut self, msg: CommitLogPageMsg) {
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
            // Prepend bypasses `set_commits`, so the filter cache must be
            // refreshed manually so an active search query still resolves
            // against the newly merged head commits.
            self.log_view.recompute_commit_filter();
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
}