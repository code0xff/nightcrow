use crate::git::diff::ChangedFile;
use crate::ui::SearchQuery;
use crate::ui::hot_stage::next_hot_deadline as next_hot_transition;
use std::cell::Cell;
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Copy)]
struct HotDeadlineCache {
    hot_window_secs: u64,
    observed_at: SystemTime,
    deadline: Option<SystemTime>,
}

#[derive(Default)]
pub struct StatusView {
    pub files: Vec<ChangedFile>,
    pub selected: usize,
    pub file_scroll_x: usize,
    pub search_query: SearchQuery,
    pub search_active: bool,
    /// Indices into `files` matching `search_query`. Recomputed only when
    /// `files` or the query changes (see `App::recompute_status_filter`).
    pub(crate) filter_cache: Vec<usize>,
    /// Per-file mtime observed at the latest snapshot, keyed by `path`, used
    /// by the agent-aware focus indicator to decide whether a file is "hot".
    /// Entries for paths missing from the latest snapshot are dropped each
    /// tick so the map stays bounded by the working-tree change count.
    pub hot_table: HashMap<String, SystemTime>,
    /// Memoized longest-path char width, keyed by `files.len()`. Used by
    /// `upper_scroll_x_max` so the right-arrow keystroke does not walk every
    /// path on every press. Length-keyed invalidation alone misses
    /// same-length content swaps (e.g. renames), so mutations must go through
    /// `set_files`, which clears this cell on every assignment.
    pub(crate) path_width_cache: Cell<Option<(usize, usize)>>,
    /// Memoized earliest hot-stage transition over the filtered rows. The
    /// event loop asks for it every tick, so recomputing it per tick would
    /// walk the whole file list; it is dropped when the files, filter, or hot
    /// table change and once the stored deadline passes.
    hot_deadline_cache: Cell<Option<HotDeadlineCache>>,
}

impl StatusView {
    /// Replace `files` and invalidate the path-width cache so a same-length
    /// snapshot whose contents changed (e.g. a file rename) does not leave a
    /// stale right-scroll bound.
    pub(crate) fn set_files(&mut self, files: Vec<ChangedFile>) {
        self.files = files;
        self.path_width_cache.set(None);
        self.invalidate_hot_deadline();
    }

    pub fn clear_search(&mut self) {
        self.search_query.clear();
    }

    /// Refresh `filter_cache` from `files` and the current query. Callers
    /// must invoke this after mutating `files` or `search_query`.
    pub(crate) fn recompute_filter(&mut self) {
        self.invalidate_hot_deadline();
        self.filter_cache.clear();
        if self.search_query.is_empty() {
            self.filter_cache.extend(0..self.files.len());
            return;
        }
        let q = self.search_query.lower();
        for (i, f) in self.files.iter().enumerate() {
            if f.search_lower.contains(q) {
                self.filter_cache.push(i);
            }
        }
    }

    pub fn start_search(&mut self) {
        self.search_active = true;
    }

    pub(crate) fn invalidate_hot_deadline(&self) {
        self.hot_deadline_cache.set(None);
    }

    /// Earliest stage transition among the filtered rows, memoized until the
    /// deadline passes or the inputs are invalidated.
    pub(crate) fn next_hot_deadline(
        &self,
        now: SystemTime,
        hot_window: Duration,
    ) -> Option<SystemTime> {
        let hot_window_secs = hot_window.as_secs();
        if let Some(cached) = self.hot_deadline_cache.get()
            && cached.hot_window_secs == hot_window_secs
            && now >= cached.observed_at
            && cached.deadline.is_none_or(|deadline| now < deadline)
        {
            return cached.deadline;
        }
        let deadline = self.compute_hot_deadline(now, hot_window);
        self.hot_deadline_cache.set(Some(HotDeadlineCache {
            hot_window_secs,
            observed_at: now,
            deadline,
        }));
        deadline
    }

    fn compute_hot_deadline(&self, now: SystemTime, hot_window: Duration) -> Option<SystemTime> {
        self.filter_cache
            .iter()
            .filter_map(|&idx| self.files.get(idx))
            .filter_map(|file| self.hot_table.get(&file.path))
            .filter_map(|mtime| next_hot_transition(*mtime, now, hot_window))
            .min()
    }

    /// Exit the search bar and clear any active query. Always recomputes the
    /// filter so the caller can refresh the diff against the now-unfiltered
    /// list without inspecting prior state.
    pub fn cancel_search(&mut self) {
        self.search_active = false;
        self.clear_search();
        self.recompute_filter();
    }

    /// Hide the search bar. Returns `true` when the query was empty and the
    /// call collapsed to a cancel (the caller should refresh the diff so a
    /// stale selection from the empty-filter state is re-pinned).
    pub fn confirm_search(&mut self) -> bool {
        if self.search_query.is_empty() {
            self.cancel_search();
            true
        } else {
            self.search_active = false;
            false
        }
    }

    pub fn search_push(&mut self, ch: char) {
        self.search_query.push(ch);
        self.recompute_filter();
    }

    pub fn search_pop(&mut self) {
        self.search_query.pop();
        self.recompute_filter();
    }
}

#[derive(Default, Clone)]
pub struct RepoInput {
    pub active: bool,
    pub buf: String,
    /// Directory names offered by the last Tab press, shown on the hint row.
    /// Any edit clears them: the list describes a fragment that no longer
    /// matches what is in the buffer.
    pub candidates: Vec<String>,
    /// The directory browser, when open. `Some` takes over the body and the
    /// dialog's keys; the field stays on screen below it and keeps the text, so
    /// closing the browser returns to exactly what was being typed.
    pub picker: Option<crate::workspace::PathTree>,
    /// Monotonic phase for the short focus cue shown when the dialog opens.
    pub(crate) focus_flash_started: Option<Instant>,
    pub(crate) focus_flash_phase: u8,
}

/// One bright/dim half-cycle of the repo-input focus cue.
pub(crate) const REPO_INPUT_FLASH_PHASE: Duration = Duration::from_millis(100);
const REPO_INPUT_FLASH_PHASES: u8 = 4;

impl RepoInput {
    pub(crate) fn start_focus_flash(&mut self) {
        self.start_focus_flash_at(Instant::now());
    }

    pub(crate) fn start_focus_flash_at(&mut self, now: Instant) {
        self.focus_flash_started = Some(now);
        self.focus_flash_phase = 0;
    }

    /// Move to the phase containing `now`; returns whether a repaint is needed.
    pub(crate) fn advance_focus_flash_at(&mut self, now: Instant) -> bool {
        let Some(started) = self.focus_flash_started else {
            return false;
        };
        let phase = (now.saturating_duration_since(started).as_millis()
            / REPO_INPUT_FLASH_PHASE.as_millis())
        .min(u128::from(REPO_INPUT_FLASH_PHASES)) as u8;
        let changed = phase != self.focus_flash_phase;
        self.focus_flash_phase = phase;
        if phase >= REPO_INPUT_FLASH_PHASES {
            self.focus_flash_started = None;
        }
        changed
    }

    pub(crate) fn focus_flash_bright(&self) -> bool {
        self.focus_flash_started.is_some() && self.focus_flash_phase.is_multiple_of(2)
    }
}
