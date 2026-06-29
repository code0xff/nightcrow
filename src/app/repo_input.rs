use super::{App, Focus, SnapshotChannel, ViewMode};

// Mirrors `PROMPT_BUFFER_MAX_BYTES` so a bracketed paste cannot grow this
// buffer without bound; comfortably above any realistic filesystem path.
const REPO_INPUT_MAX_BYTES: usize = 4096;

impl App {
    pub fn change_repo(&mut self, new_path: String) {
        // Drop any commit-log page worker tied to the previous repo so its
        // result (built against the old `.git`) cannot leak into the new view.
        self.cancel_commit_log_page_fetch();
        // Replacing the channel drops the prior SnapshotChannel; its `Drop`
        // signals and joins the old-repo worker before this assignment
        // returns, so no in-flight load_snapshot leaks into the new state.
        self.snapshot = SnapshotChannel::spawn(&new_path);
        if let Some(ref mut backend) = self.terminal.backend {
            // Only future panes adopt the new cwd; existing shells stay in
            // their original directory so we don't disrupt commands already
            // running in them. Users who want the new cwd everywhere can
            // close existing panes (ctrl+w) and open fresh ones (ctrl+t).
            backend.set_cwd(std::path::Path::new(&new_path));
        }
        tracing::info!(path = %new_path, "repo changed");
        self.repo_path = new_path;
        // Drop the cached Repository — it points to the previous repo's .git
        // directory and would silently keep returning stale results.
        self.repo_cache = None;
        self.mode = ViewMode::Status;
        // Go through `set_files` / `set_commits` so the width caches stay
        // in lockstep with the underlying vec — manual `.clear()` calls
        // would drift if the setter contract grows new invariants.
        self.status_view.set_files(Vec::new());
        self.status_view.selected = 0;
        self.status_view.file_scroll_x = 0;
        // Hot mtimes are workdir-scoped; carrying them into the new repo would
        // bias auto-follow toward unrelated paths until the first snapshot tick.
        self.status_view.hot_table.clear();
        self.log_view.set_commits(Vec::new());
        self.log_view.selected = 0;
        self.log_view.diff_title.clear();
        self.log_view.commit_scroll_x = 0;
        // `reset_drill_down` also clears `commit_files` and its width cache.
        self.log_view.reset_drill_down();
        // Tree cache/expansion/selection are workdir-scoped; drop them so the
        // new repo's tree starts fresh (and never previews a stale path).
        self.tree_view.reset();
        // The watcher holds absolute paths under the *old* workdir; replace it
        // with a fresh one so no stale watch survives the switch (respecting the
        // live-watch setting). The next Tree entry re-syncs it against the new
        // repo's expansion set.
        self.tree_watch = if self.cfg_tree.live_watch {
            crate::runtime::tree_watch::TreeWatcher::new()
        } else {
            crate::runtime::tree_watch::TreeWatcher::disabled()
        };
        self.status_view.cancel_search();
        // clear_diff_state empties hunks + lower/highlight caches, resets diff
        // scroll/search cursor, drops the search query, and invalidates the
        // open file view. Calling it here keeps the per-load reset shape
        // centralized.
        self.clear_diff_state();
        // Auto-follow timers and steered-path memory are tied to the previous
        // workdir; reset them so the new repo's first snapshot starts clean.
        self.auto_follow.last_manual_nav_at = None;
        self.auto_follow.followed_path = None;
        self.status = None;
        self.tracking = None;
        self.focus = Focus::FileList;
        // Drop transient view modes — the previous repo's diff zoom, terminal
        // fullscreen, or list fullscreen has no meaning under the new working tree.
        self.diff.fullscreen = false;
        self.terminal.fullscreen = false;
        self.list_fullscreen = false;
        // Drop any pending session restore for the previous repo. Without this,
        // a Ctrl+O before the first snapshot of the old repo lands would have
        // its saved focus/fullscreen/selection applied to the new repo via
        // `ingest_snapshot`, overriding the explicit reset above.
        self.pending_session = None;
        // The new repo's first snapshot will populate `last_head_oid` and
        // skip the reload branch (initial snapshot guard). Keeping the prior
        // repo's HEAD here would otherwise trigger a spurious commit log
        // reload for the new repo.
        self.pagination.last_head_oid = None;
        // Branch label is workdir-scoped; clearing here prevents the previous
        // repo's branch from flashing in the header until the first snapshot
        // of the new repo arrives.
        self.branch_name = None;
    }

    pub fn start_repo_input(&mut self) {
        self.repo_input.buf = self.repo_path.clone();
        self.repo_input.active = true;
    }

    pub fn cancel_repo_input(&mut self) {
        self.repo_input.active = false;
        self.repo_input.buf.clear();
    }

    pub fn confirm_repo_input(&mut self) {
        // Validate against the live buffer so a failed attempt leaves the
        // dialog open with the user's text intact for correction; only close
        // and consume the buffer once we're committed to switching repos.
        let trimmed = self.repo_input.buf.trim();
        if trimmed.is_empty() {
            self.status = Some("repo path cannot be empty".to_string());
            return;
        }
        let p = std::path::Path::new(trimmed);
        if !p.is_dir() {
            self.status = Some(format!("not a directory: {trimmed}"));
            return;
        }
        let resolved = crate::git::resolve_repo_path(p)
            .to_string_lossy()
            .to_string();
        self.repo_input.active = false;
        self.repo_input.buf.clear();
        self.change_repo(resolved);
    }

    pub fn repo_input_push(&mut self, ch: char) {
        if self.repo_input.buf.len() + ch.len_utf8() > REPO_INPUT_MAX_BYTES {
            return;
        }
        self.repo_input.buf.push(ch);
    }

    pub fn repo_input_pop(&mut self) {
        self.repo_input.buf.pop();
    }
}
