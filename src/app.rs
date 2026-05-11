use crate::backend::BackendEvent;
use crate::backend::{PaneId, PtyBackend, TerminalBackend};
use crate::git::diff::{
    ChangedFile, CommitEntry, DiffHunk, LineKind, RepoSnapshot, TrackingStatus, load_commit_diff,
    load_commit_file_blob, load_commit_file_diff, load_commit_files, load_commit_log,
    load_file_diff, load_snapshot, load_workdir_file, parse_hunk_new_start,
};
use std::cell::Cell;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

const SCROLLBACK_LINES: usize = 1000;
const LIST_PAGE_SIZE: usize = 10;
const DIFF_PAGE_SIZE: usize = 20;
const COMMIT_LOG_LIMIT: usize = 500;

/// Move a list index up by `n`, saturating at 0. Returns `true` when the index
/// actually changed so callers can decide whether to refresh associated state.
fn cursor_up(idx: &mut usize, n: usize) -> bool {
    let next = idx.saturating_sub(n);
    if next != *idx {
        *idx = next;
        true
    } else {
        false
    }
}

/// Move a list index down by `n`, clamped to `len - 1`. Returns `true` when the
/// index actually changed. A zero-length list is a no-op.
fn cursor_down(idx: &mut usize, len: usize, n: usize) -> bool {
    if len == 0 {
        return false;
    }
    let next = idx.saturating_add(n).min(len - 1);
    if next != *idx {
        *idx = next;
        true
    } else {
        false
    }
}

/// Owns the receiver and stop channel for the background snapshot thread.
/// Dropping the struct (and its `_stop_tx`) signals the thread to exit.
pub struct SnapshotChannel {
    rx: Receiver<SnapshotMsg>,
    // Held only for its drop side-effect: dropping the sender unblocks
    // the worker's recv_timeout so it can observe the disconnect.
    _stop_tx: SyncSender<()>,
}

/// Reopen the cached `git2::Repository` handle every N ticks so we observe
/// out-of-band repo changes (e.g. `git gc`, packfile rewrites, worktree
/// moves) that the cached handle would otherwise serve stale. ~30 s at the
/// current 1 s tick is cheap and predictable.
const REOPEN_REPO_EVERY_TICKS: u32 = 30;

impl SnapshotChannel {
    pub fn spawn(repo_path: &str) -> Self {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, stop_rx) = mpsc::sync_channel::<()>(0);
        let path = repo_path.to_string();
        thread::spawn(move || {
            // Cache the Repository handle to avoid a fresh `discover` walk
            // every tick, but drop it periodically (and on any load error)
            // so external repo mutations cannot leave us serving stale state.
            let mut repo: Option<git2::Repository> = None;
            let mut ticks_since_open: u32 = 0;
            loop {
                if ticks_since_open >= REOPEN_REPO_EVERY_TICKS {
                    repo = None;
                }
                if repo.is_none() {
                    match git2::Repository::discover(&path) {
                        Ok(r) => {
                            repo = Some(r);
                            ticks_since_open = 0;
                        }
                        Err(e) => {
                            let msg = SnapshotMsg::Err(format!("not a git repository: {e}"));
                            if tx.send(msg).is_err() {
                                break;
                            }
                            match stop_rx.recv_timeout(Duration::from_millis(1000)) {
                                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                                Err(mpsc::RecvTimeoutError::Timeout) => {}
                            }
                            continue;
                        }
                    }
                }
                let r = repo.as_ref().expect("repo just opened");
                let msg = match load_snapshot(r) {
                    Ok(s) => {
                        let mtimes = r
                            .workdir()
                            .map(|w| collect_mtimes(w, &s))
                            .unwrap_or_default();
                        SnapshotMsg::Ok(s, mtimes)
                    }
                    Err(e) => {
                        // Drop the handle: the next tick will re-discover.
                        // This covers the case where the repo was relocated
                        // or its internal state became inconsistent.
                        repo = None;
                        SnapshotMsg::Err(e.to_string())
                    }
                };
                ticks_since_open += 1;
                if tx.send(msg).is_err() {
                    break;
                }
                match stop_rx.recv_timeout(Duration::from_millis(1000)) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
        });
        Self {
            rx,
            _stop_tx: stop_tx,
        }
    }

    pub fn try_recv(&self) -> Result<SnapshotMsg, mpsc::TryRecvError> {
        self.rx.try_recv()
    }
}

fn strip_escape_sequences(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\x1b' => match chars.peek().copied() {
                Some('[') => {
                    // CSI: consume parameter/intermediate bytes (0x20–0x3f), stop at
                    // final byte (0x40–0x7e). Break early on control chars to avoid
                    // consuming content that follows a malformed sequence.
                    chars.next();
                    for c in chars.by_ref() {
                        if ('\x40'..='\x7e').contains(&c) {
                            break;
                        }
                        if c < '\x20' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    // OSC: skip until BEL or ST
                    chars.next();
                    loop {
                        match chars.next() {
                            None | Some('\x07') => break,
                            Some('\x1b') if chars.peek() == Some(&'\\') => {
                                chars.next();
                                break;
                            }
                            _ => {}
                        }
                    }
                }
                Some('O') => {
                    // SS3: ESC O <final>. Used by xterm-style application
                    // keypad for arrow/function keys. Consume the `O`, then
                    // only consume the next char when it looks like a valid
                    // SS3 final byte (0x40–0x7e). A malformed `ESC O <x>`
                    // sequence followed by ordinary text used to swallow `x`.
                    chars.next();
                    if let Some(&next) = chars.peek()
                        && ('\x40'..='\x7e').contains(&next)
                    {
                        chars.next();
                    }
                }
                Some('(') | Some(')') | Some('*') | Some('+') | Some('-') | Some('.')
                | Some('/') | Some('#') => {
                    // Charset designators / DEC private 2-byte escapes:
                    // ESC <intermediate> <final>. Skip both.
                    chars.next();
                    chars.next();
                }
                _ => {
                    // Drop the bare ESC and let the next iteration process
                    // whatever follows as ordinary input. Consuming an extra
                    // byte here would silently swallow user keystrokes that
                    // happened to land right after a stray Esc.
                }
            },
            '\r' | '\n' => result.push(ch),
            c if !c.is_control() => result.push(c),
            _ => {}
        }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub enum ViewMode {
    #[default]
    Status,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Focus {
    FileList,
    DiffViewer,
    Terminal,
}

pub enum SnapshotMsg {
    Ok(RepoSnapshot, HashMap<String, SystemTime>),
    Err(String),
}

/// Stat every file in `snapshot` against `repo_root` and return its mtime.
/// Files that cannot be stat'd (deleted between snapshot and stat) are
/// dropped; absence in the returned map removes them from `hot_table`.
/// Runs on the snapshot worker thread to keep filesystem syscalls off the
/// UI thread.
fn collect_mtimes(repo_root: &Path, snapshot: &RepoSnapshot) -> HashMap<String, SystemTime> {
    let mut out = HashMap::with_capacity(snapshot.files.len());
    for f in &snapshot.files {
        if let Ok(meta) = std::fs::metadata(repo_root.join(&f.path))
            && let Ok(mtime) = meta.modified()
        {
            out.insert(f.path.clone(), mtime);
        }
    }
    out
}

pub struct PaneInfo {
    pub id: PaneId,
    pub title: String,
}

pub struct TerminalState {
    pub panes: Vec<PaneInfo>,
    pub active: usize,
    pub size: (u16, u16),
    pub scroll: HashMap<PaneId, usize>,
    pub fullscreen: bool,
    parsers: HashMap<PaneId, vt100::Parser>,
    prompt_bufs: HashMap<PaneId, String>,
    prompt_log_enabled: bool,
    backend: Option<Box<dyn TerminalBackend>>,
}

impl TerminalState {
    pub fn active_pane_id(&self) -> Option<PaneId> {
        self.panes.get(self.active).map(|p| p.id)
    }

    pub fn scroll_up(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(id) = self.active_pane_id() {
            let offset = self.scroll.entry(id).or_insert(0);
            *offset = offset.saturating_add(lines);
        }
    }

    pub fn scroll_down(&mut self, lines: usize) {
        if lines == 0 {
            return;
        }
        if let Some(id) = self.active_pane_id()
            && let Some(entry) = self.scroll.get_mut(&id)
        {
            *entry = entry.saturating_sub(lines);
            if *entry == 0 {
                self.scroll.remove(&id);
            }
        }
    }

    pub fn is_scrolled(&self) -> bool {
        self.active_pane_id()
            .and_then(|id| self.scroll.get(&id))
            .is_some_and(|&v| v > 0)
    }

    pub fn sync_scroll(&mut self) {
        let Some(id) = self.active_pane_id() else {
            return;
        };
        let offset = self.scroll.get(&id).copied().unwrap_or(0);
        let actual = match self.parsers.get_mut(&id) {
            Some(parser) => {
                // vt100 clamps the offset to the actual scrollback
                // buffer size internally, so we can pass the full request
                // through and read back what was applied.
                parser.screen_mut().set_scrollback(offset);
                parser.screen().scrollback()
            }
            None => return,
        };
        if actual == 0 {
            self.scroll.remove(&id);
        } else {
            self.scroll.insert(id, actual);
        }
    }

    fn buffer_prompt_input(&mut self, pane_id: PaneId, data: &[u8]) {
        let text = strip_escape_sequences(data);
        let buf = self.prompt_bufs.entry(pane_id).or_default();
        for ch in text.chars() {
            if ch == '\r' || ch == '\n' {
                if !buf.is_empty() {
                    tracing::info!(target: "prompt", pane = pane_id, text = %buf);
                    buf.clear();
                }
            } else {
                buf.push(ch);
            }
        }
    }

    pub fn resize_panes(&mut self, rows: u16, cols: u16) {
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        let r = rows.max(1);
        let c = cols.max(1);
        for info in &self.panes {
            if let Some(backend) = &mut self.backend {
                backend.resize(info.id, r, c);
            }
            if let Some(parser) = self.parsers.get_mut(&info.id) {
                parser.screen_mut().set_size(r, c);
            }
        }
    }

    pub fn send_input(&mut self, data: &[u8]) {
        let Some(info) = self.panes.get(self.active) else {
            return;
        };
        let id = info.id;
        self.scroll.remove(&id);
        if let Some(backend) = &mut self.backend
            && let Err(e) = backend.send_input(id, data)
        {
            tracing::warn!("failed to send terminal input to pane {id}: {e}");
        }
        if self.prompt_log_enabled {
            self.buffer_prompt_input(id, data);
        }
    }

    pub fn new(backend: Option<Box<dyn TerminalBackend>>, prompt_log_enabled: bool) -> Self {
        Self {
            panes: Vec::new(),
            active: 0,
            size: (22, 78),
            scroll: HashMap::new(),
            fullscreen: false,
            parsers: HashMap::new(),
            prompt_bufs: HashMap::new(),
            prompt_log_enabled,
            backend,
        }
    }
}

#[derive(Default)]
pub struct StatusView {
    pub files: Vec<ChangedFile>,
    pub selected: usize,
    pub file_scroll_x: usize,
    pub search_query: String,
    search_query_lower: String,
    pub search_active: bool,
    /// Indices into `files` that match the current `search_query`.
    /// Recomputed only when `files` or the query changes (see
    /// `App::recompute_status_filter`). Read-only for renderers.
    filter_cache: Vec<usize>,
    /// Per-file mtime observed at the latest snapshot, keyed by `path`.
    /// Used by the agent-aware focus indicator to decide whether a file
    /// is currently "hot" (recently touched). Entries for paths missing
    /// from the latest snapshot are dropped each tick so the map stays
    /// bounded by the working-tree change count.
    pub hot_table: HashMap<String, SystemTime>,
    /// Memoized longest-path char width, keyed by `files.len()`. Used by
    /// `upper_scroll_x_max` so the right-arrow keystroke does not walk every
    /// path on every press. Invalidated on length change; in this app the
    /// snapshot worker replaces `files` wholesale every tick so length-keyed
    /// invalidation is reliable enough for scroll bounds.
    path_width_cache: Cell<Option<(usize, usize)>>,
}

impl StatusView {
    /// Clear the search query and its lowercase cache together so callers
    /// can't accidentally reset only one and leave the cache stale.
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_query_lower.clear();
    }

    /// Refresh `filter_cache` from `files` and the current query. Callers must
    /// invoke this after mutating `files`, `search_query`, or
    /// `search_query_lower`; otherwise the cache will diverge from state.
    fn recompute_filter(&mut self) {
        self.filter_cache.clear();
        if self.search_query.is_empty() {
            self.filter_cache.extend(0..self.files.len());
            return;
        }
        let q = self.search_query_lower.as_str();
        for (i, f) in self.files.iter().enumerate() {
            if f.path_lower.contains(q) {
                self.filter_cache.push(i);
            }
        }
    }

    pub fn start_search(&mut self) {
        self.search_active = true;
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
    /// call therefore collapsed to a cancel (the caller should refresh the
    /// diff in that case so a stale selection from the empty-filter state is
    /// re-pinned).
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
        self.search_query_lower = self.search_query.to_lowercase();
        self.recompute_filter();
    }

    pub fn search_pop(&mut self) {
        self.search_query.pop();
        self.search_query_lower = self.search_query.to_lowercase();
        self.recompute_filter();
    }
}

#[derive(Default)]
pub struct RepoInput {
    pub active: bool,
    pub buf: String,
}

/// Post-load behaviour for `apply_diff_result`. Replaces the prior 3-flag
/// signature where the combination of `reset_scroll` and `keep_scroll` was
/// hard to parse at call sites.
enum DiffApply<'a> {
    /// Reset scroll/cursor to top after a successful load.
    Reset,
    /// Keep the previous scroll position (for in-place refresh).
    KeepScroll(usize),
    /// Reset scroll and additionally update the log diff title.
    ResetWithTitle(&'a str),
}

#[derive(Default)]
pub struct LogView {
    pub commits: Vec<CommitEntry>,
    pub selected: usize,
    pub diff_title: String,
    pub drill_down: bool,
    pub commit_files: Vec<ChangedFile>,
    pub file_selected: usize,
    pub commit_scroll_x: usize,
    pub file_scroll_x: usize,
    /// Memoized longest-summary char width, keyed by `commits.len()`. See
    /// `StatusView::path_width_cache` for the invalidation contract.
    commit_width_cache: Cell<Option<(usize, usize)>>,
    /// Memoized longest-path char width for `commit_files`.
    commit_files_width_cache: Cell<Option<(usize, usize)>>,
}

impl LogView {
    /// Exit drill-down so the upper pane shows the commit list again. Clears
    /// the file list and resets file-side cursors/scroll so a later drill-in
    /// starts from a clean state.
    pub fn reset_drill_down(&mut self) {
        self.drill_down = false;
        self.commit_files.clear();
        self.file_selected = 0;
        self.file_scroll_x = 0;
    }

    /// Move the file-list cursor up by `n`. Returns whether the selection
    /// actually changed so the caller can decide whether to reload the diff.
    /// A non-zero move also resets `file_scroll_x` to mirror the established
    /// behaviour of clearing horizontal scroll when the highlighted row moves.
    pub fn file_select_up(&mut self, n: usize) -> bool {
        let moved = cursor_up(&mut self.file_selected, n);
        if moved {
            self.file_scroll_x = 0;
        }
        moved
    }

    /// Move the file-list cursor down by `n`. See `file_select_up` for the
    /// return-value contract.
    pub fn file_select_down(&mut self, n: usize) -> bool {
        let moved = cursor_down(&mut self.file_selected, self.commit_files.len(), n);
        if moved {
            self.file_scroll_x = 0;
        }
        moved
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffPaneView {
    #[default]
    Diff,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileViewKey {
    Status(String),
    Commit { oid: git2::Oid, path: String },
}

#[derive(Default)]
pub struct FileViewState {
    pub key: Option<FileViewKey>,
    pub content: String,
    pub scroll: usize,
    pub scroll_x: usize,
    pub anchor_line: Option<usize>,
    pub error: Option<String>,
    /// Cached syntect highlight output, one entry per `content.lines()` line.
    /// Built once per (content, syntax) so per-frame rendering only slices the
    /// visible window instead of re-highlighting the whole file.
    pub line_highlights: Vec<Vec<HighlightSegment>>,
    /// Syntax name used to build `line_highlights`. `None` means the cache is
    /// unbuilt or invalidated (e.g. on content reload).
    pub cached_syntax_name: Option<String>,
    /// Cached `content.lines().count()` populated on load. Avoids walking the
    /// full file on every scroll keystroke (`FileViewState::max_scroll` is called
    /// from each j/k/PgUp/PgDn handler).
    total_lines: usize,
    /// Byte length of `content` at the time `line_highlights` was built.
    /// Combined with `total_lines` it lets `ensure_highlight_cache` notice
    /// in-place content edits that happen to keep the line count constant
    /// (line counts alone are too coarse a fingerprint).
    cached_content_len: usize,
}

impl FileViewState {
    pub fn line_count(&self) -> usize {
        self.total_lines
    }

    /// Largest legal `scroll` value: one less than `line_count`, or 0 when
    /// the file is empty.
    pub fn max_scroll(&self) -> usize {
        self.line_count().saturating_sub(1)
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_add(n).min(self.max_scroll());
    }

    /// Ensure `line_highlights` matches the current `content` and supplied
    /// syntax. Rebuilds when the line count diverges or the syntax name
    /// changed since the last build. Builds once per file load; the renderer
    /// only slices the visible window from the cache afterward.
    pub fn ensure_highlight_cache(
        &mut self,
        ss: &syntect::parsing::SyntaxSet,
        ts: &syntect::highlighting::ThemeSet,
        syntax: &syntect::parsing::SyntaxReference,
    ) {
        let total = self.line_count();
        let content_len = self.content.len();
        if self.line_highlights.len() == total
            && self.cached_content_len == content_len
            && self.cached_syntax_name.as_deref() == Some(syntax.name.as_str())
        {
            return;
        }

        use syntect::easy::HighlightLines;
        let theme = &ts.themes[DIFF_THEME];
        let mut hl = HighlightLines::new(syntax, theme);

        let mut out: Vec<Vec<HighlightSegment>> = Vec::with_capacity(total);
        for raw in self.content.lines() {
            let with_nl = format!("{raw}\n");
            let segs: Vec<HighlightSegment> = match hl.highlight_line(&with_nl, ss) {
                Ok(ranges) => ranges
                    .into_iter()
                    .filter_map(|(style, text)| {
                        let trimmed = text.trim_end_matches('\n');
                        if trimmed.is_empty() {
                            return None;
                        }
                        Some(HighlightSegment {
                            rgb: (style.foreground.r, style.foreground.g, style.foreground.b),
                            text: trimmed.to_string(),
                        })
                    })
                    .collect(),
                Err(_) => vec![HighlightSegment {
                    rgb: (200, 200, 200),
                    text: raw.to_string(),
                }],
            };
            out.push(segs);
        }
        self.line_highlights = out;
        self.cached_syntax_name = Some(syntax.name.clone());
        self.cached_content_len = content_len;
    }
}

#[derive(Default)]
pub struct DiffSearch {
    pub active: bool,
    pub query: String,
    query_lower: String,
    pub matches: Vec<usize>,
    pub cursor: usize,
}

impl DiffSearch {
    pub fn is_visible(&self) -> bool {
        self.active || !self.query.is_empty()
    }

    pub fn has_query(&self) -> bool {
        !self.query.is_empty()
    }

    pub fn current_match(&self) -> Option<usize> {
        self.matches.get(self.cursor).copied()
    }

    pub fn is_match(&self, flat_idx: usize) -> bool {
        // `matches` is built by `recompute_diff_matches` in flat_idx-ascending
        // order, so binary_search is always sound here.
        self.matches.binary_search(&flat_idx).is_ok()
    }

    fn start(&mut self) {
        self.active = true;
    }

    fn confirm(&mut self) {
        if self.query.is_empty() {
            self.clear();
        } else {
            self.active = false;
        }
    }

    fn clear(&mut self) {
        self.active = false;
        self.query.clear();
        self.query_lower.clear();
        self.matches.clear();
        self.cursor = 0;
    }

    fn push_char(&mut self, ch: char) {
        self.query.push(ch);
        self.query_lower = self.query.to_lowercase();
    }

    fn pop_char(&mut self) {
        self.query.pop();
        self.query_lower = self.query.to_lowercase();
    }

    fn next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        self.cursor = (self.cursor + 1) % self.matches.len();
        self.current_match()
    }

    fn prev(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        if self.cursor == 0 {
            self.cursor = self.matches.len() - 1;
        } else {
            self.cursor -= 1;
        }
        self.current_match()
    }
}

/// Syntect theme name used for both the diff and file-view highlight caches.
pub const DIFF_THEME: &str = "base16-ocean.dark";

/// One highlighted segment of a body line: foreground RGB + the text.
/// Cached so per-frame rendering does not re-run the syntect highlighter
/// over the whole document for state recovery.
#[derive(Debug, Clone)]
pub struct HighlightSegment {
    pub rgb: (u8, u8, u8),
    pub text: String,
}

/// All state for the diff viewer pane: the loaded hunks, scroll cursors,
/// search state, and the optional file-content overlay. Lifted out of App
/// so renderers and navigation handlers operate on a self-contained value.
#[derive(Default)]
pub struct DiffPane {
    pub hunks: Vec<DiffHunk>,
    /// Lowercased copy of each `DiffLine::content` aligned with `hunks`.
    /// `hunks_lines_lower[i][j]` corresponds to `hunks[i].lines[j].content`.
    /// Built once per diff load so per-keystroke search does not re-lowercase
    /// the entire diff. Header lines are never searched and are not cached.
    hunks_lines_lower: Vec<Vec<String>>,
    /// Cached syntect highlight output per body line. Same shape as
    /// `hunks_lines_lower`. Built once when hunks (or the active syntax)
    /// change so the renderer skips the full-document state-recovery pass
    /// every frame.
    pub line_highlights: Vec<Vec<Vec<HighlightSegment>>>,
    /// Syntax name (`SyntaxReference::name`) used to build `line_highlights`.
    /// `None` means the cache is unbuilt or invalidated.
    pub cached_syntax_name: Option<String>,
    /// Sum of `line.content.len()` across all hunk lines at the time
    /// `line_highlights` was built. Pairs with the shape check so a hunk
    /// replacement that happens to preserve the same line counts still
    /// invalidates the cache. Belt-and-braces on top of the existing
    /// `rebuild_diff_lower_cache` invariant.
    cached_content_bytes: usize,
    pub scroll: usize,
    pub scroll_x: usize,
    pub search: DiffSearch,
    pub view: DiffPaneView,
    pub file_view: FileViewState,
    /// True while the diff pane is rendered full-screen (hint bar excluded).
    /// Toggled by `Ctrl+F` while focus is on `DiffViewer`; mutually exclusive
    /// with `TerminalPane::fullscreen`.
    pub fullscreen: bool,
}

impl DiffPane {
    /// Total flat row count across all hunks (1 header + N body lines each).
    pub fn line_count(&self) -> usize {
        self.hunks.iter().map(|h| 1 + h.lines.len()).sum()
    }

    /// Largest legal `scroll` value: one less than the total row count, or 0
    /// when there are no rows. Callers clamp restored scroll positions and
    /// page-down ends against this bound.
    pub fn max_scroll(&self) -> usize {
        self.line_count().saturating_sub(1)
    }

    /// Move the active horizontal scroll target (diff or file view, depending
    /// on `self.view`) left by one tab stop.
    pub fn scroll_left(&mut self) {
        let target = self.scroll_x_target_mut();
        *target = target.saturating_sub(4);
    }

    /// Move the active horizontal scroll target right by one tab stop.
    /// Capped at `u16::MAX` because ratatui's `Paragraph::scroll` takes `u16`.
    pub fn scroll_right(&mut self) {
        let target = self.scroll_x_target_mut();
        *target = target.saturating_add(4).min(u16::MAX as usize);
    }

    fn scroll_x_target_mut(&mut self) -> &mut usize {
        match self.view {
            DiffPaneView::File => &mut self.file_view.scroll_x,
            DiffPaneView::Diff => &mut self.scroll_x,
        }
    }

    pub fn start_search(&mut self) {
        self.search.start();
    }

    pub fn cancel_search(&mut self) {
        self.search.clear();
    }

    pub fn confirm_search(&mut self) {
        self.search.confirm();
    }

    pub fn search_push(&mut self, ch: char) {
        self.search.push_char(ch);
        self.recompute_matches(true);
    }

    pub fn search_pop(&mut self) {
        self.search.pop_char();
        self.recompute_matches(true);
    }

    pub fn next_match(&mut self) {
        if let Some(idx) = self.search.next() {
            self.scroll = idx;
        }
    }

    pub fn prev_match(&mut self) {
        if let Some(idx) = self.search.prev() {
            self.scroll = idx;
        }
    }

    /// Rebuild `search.matches` against the current query, using
    /// `hunks_lines_lower` so per-keystroke search is just a substring scan
    /// over precomputed strings.
    pub fn recompute_matches(&mut self, scroll_to_match: bool) {
        self.search.matches.clear();
        if self.search.query.is_empty() {
            self.search.cursor = 0;
            return;
        }
        self.ensure_lower_cache();
        let q = self.search.query_lower.as_str();
        let mut flat_idx = 0usize;
        for (hunk, lines_lower) in self.hunks.iter().zip(self.hunks_lines_lower.iter()) {
            flat_idx += 1; // header line
            for line_lower in lines_lower.iter().take(hunk.lines.len()) {
                if line_lower.contains(q) {
                    self.search.matches.push(flat_idx);
                }
                flat_idx += 1;
            }
        }
        debug_assert!(
            self.search.matches.windows(2).all(|w| w[0] < w[1]),
            "diff_search_matches must be sorted for binary_search to be correct"
        );
        if !self.search.matches.is_empty() {
            self.search.cursor = self
                .search
                .cursor
                .min(self.search.matches.len().saturating_sub(1));
            if scroll_to_match {
                self.scroll_to_match();
            }
        } else {
            self.search.cursor = 0;
        }
    }

    fn scroll_to_match(&mut self) {
        if let Some(&idx) = self.search.matches.get(self.search.cursor) {
            self.scroll = idx;
        }
    }

    /// Rebuild the lowercased line cache from scratch and invalidate the
    /// highlight cache so the renderer rebuilds it on next frame.
    pub fn rebuild_lower_cache(&mut self) {
        self.hunks_lines_lower.clear();
        self.hunks_lines_lower.reserve(self.hunks.len());
        for hunk in &self.hunks {
            let lines = hunk
                .lines
                .iter()
                .map(|l| l.content.to_lowercase())
                .collect();
            self.hunks_lines_lower.push(lines);
        }
        // Highlight cache shape is keyed by hunks; invalidate so the renderer
        // rebuilds it on next frame against the active syntax.
        self.line_highlights.clear();
        self.cached_syntax_name = None;
    }

    /// Rebuild the lowercased line cache iff its shape diverges from `hunks`.
    /// Cheap path for callers that aren't sure whether the cache is current.
    pub fn ensure_lower_cache(&mut self) {
        let shape_matches = self.hunks_lines_lower.len() == self.hunks.len()
            && self
                .hunks
                .iter()
                .zip(self.hunks_lines_lower.iter())
                .all(|(h, ll)| ll.len() == h.lines.len());
        if !shape_matches {
            self.rebuild_lower_cache();
        }
    }

    /// Ensure `line_highlights` matches the current `hunks` and the supplied
    /// syntax. Rebuilds when the cache shape diverges from `hunks` or the
    /// syntax name changed since last build. The walk uses two highlight
    /// states (one for context/added, one for removed) so multi-line
    /// constructs stay coherent across hunks.
    pub fn ensure_highlight_cache(
        &mut self,
        ss: &syntect::parsing::SyntaxSet,
        ts: &syntect::highlighting::ThemeSet,
        syntax: &syntect::parsing::SyntaxReference,
    ) {
        let shape_matches = self.line_highlights.len() == self.hunks.len()
            && self
                .hunks
                .iter()
                .zip(self.line_highlights.iter())
                .all(|(h, lh)| lh.len() == h.lines.len());
        let content_bytes: usize = self
            .hunks
            .iter()
            .flat_map(|h| h.lines.iter())
            .map(|l| l.content.len())
            .sum();
        if shape_matches
            && self.cached_content_bytes == content_bytes
            && self.cached_syntax_name.as_deref() == Some(syntax.name.as_str())
        {
            return;
        }

        use syntect::easy::HighlightLines;
        let theme = &ts.themes[DIFF_THEME];
        let mut hl_new = HighlightLines::new(syntax, theme);
        let mut hl_old = HighlightLines::new(syntax, theme);

        let mut out: Vec<Vec<Vec<HighlightSegment>>> = Vec::with_capacity(self.hunks.len());
        for hunk in &self.hunks {
            let mut per_hunk: Vec<Vec<HighlightSegment>> = Vec::with_capacity(hunk.lines.len());
            for line in &hunk.lines {
                let hl = match line.kind {
                    LineKind::Removed => &mut hl_old,
                    _ => &mut hl_new,
                };
                let with_nl = format!("{}\n", line.content);
                let segs: Vec<HighlightSegment> = match hl.highlight_line(&with_nl, ss) {
                    Ok(ranges) => ranges
                        .into_iter()
                        .filter_map(|(style, text)| {
                            let trimmed = text.trim_end_matches('\n');
                            if trimmed.is_empty() {
                                return None;
                            }
                            let fg = style.foreground;
                            Some(HighlightSegment {
                                rgb: (fg.r, fg.g, fg.b),
                                text: trimmed.to_string(),
                            })
                        })
                        .collect(),
                    Err(_) => vec![HighlightSegment {
                        rgb: (200, 200, 200),
                        text: line.content.clone(),
                    }],
                };
                per_hunk.push(segs);
            }
            out.push(per_hunk);
        }
        self.line_highlights = out;
        self.cached_syntax_name = Some(syntax.name.clone());
        self.cached_content_bytes = content_bytes;
    }
}

pub struct App {
    pub mode: ViewMode,
    pub status_view: StatusView,
    pub diff: DiffPane,
    pub focus: Focus,
    pub status: Option<String>,
    pub repo_path: String,
    pub log_view: LogView,
    pub terminal: TerminalState,
    pub repo_input: RepoInput,
    pub accent_idx: usize,
    pub tracking: Option<TrackingStatus>,
    snapshot: SnapshotChannel,
    pending_session: Option<crate::session::SessionState>,
    /// Cached `git2::Repository` for synchronous loads (file diff, commit
    /// diff, file blob, commit log). Opened lazily on first use; invalidated
    /// in `change_repo`. The snapshot worker thread keeps its own handle —
    /// `git2::Repository` is `!Send` and cannot be shared.
    repo_cache: Option<git2::Repository>,
    pub cfg_agent_indicator: crate::config::AgentIndicatorConfig,
    /// Wall-clock instant of the most recent user-driven selection change in
    /// the file list. `None` means "idle since boot". Used to gate
    /// auto-follow so an active user is never hijacked.
    pub last_manual_nav_at: Option<Instant>,
    /// Path the auto-follow last steered selection to. Prevents repeatedly
    /// re-asserting selection on the same already-hot-and-selected file.
    pub auto_followed_path: Option<String>,
}

impl App {
    pub fn new(repo_path: String, prompt_log: bool) -> Self {
        let snapshot = SnapshotChannel::spawn(&repo_path);

        let backend: Box<dyn TerminalBackend> = Box::new(PtyBackend::new(&repo_path));

        let mut app = App {
            mode: ViewMode::Status,
            status_view: StatusView::default(),
            diff: DiffPane::default(),
            focus: Focus::FileList,
            status: None,
            repo_path,
            log_view: LogView::default(),
            terminal: TerminalState::new(Some(backend), prompt_log),
            repo_input: RepoInput::default(),
            accent_idx: 0,
            tracking: None,
            snapshot,
            pending_session: None,
            repo_cache: None,
            cfg_agent_indicator: crate::config::AgentIndicatorConfig::default(),
            last_manual_nav_at: None,
            auto_followed_path: None,
        };

        app.ensure_initial_terminal();
        tracing::info!(repo = %app.repo_path, "nightcrow started");
        app
    }

    fn ensure_initial_terminal(&mut self) {
        if self.terminal.backend.is_none() {
            return;
        }

        if let Err(err) = self.create_terminal_pane() {
            self.status = Some(format!("terminal error: {err}"));
        }
    }

    // ── Snapshot polling ──────────────────────────────────────────

    pub fn poll_snapshot(&mut self) {
        while let Ok(msg) = self.snapshot.try_recv() {
            match msg {
                SnapshotMsg::Ok(snapshot, mtimes) => {
                    self.ingest_snapshot(snapshot, mtimes);
                }
                SnapshotMsg::Err(e) => {
                    tracing::warn!(error = %e, "git snapshot failed");
                    self.status = Some(format!("git error: {e}"));
                }
            }
        }
    }

    /// Apply a snapshot to app state. Split out from `poll_snapshot` so
    /// tests can drive the merge/auto-follow logic with deterministic
    /// mtimes instead of booting the background worker.
    pub fn ingest_snapshot(&mut self, snapshot: RepoSnapshot, mtimes: HashMap<String, SystemTime>) {
        let previous_path = self
            .status_view
            .files
            .get(self.status_view.selected)
            .map(|f| f.path.clone());
        self.status_view.files = snapshot.files;
        self.status_view.recompute_filter();
        self.tracking = snapshot.tracking;
        self.merge_hot_table(mtimes);

        self.restore_selection(previous_path.as_deref());
        self.sync_selection_to_filter();
        let auto_followed = self.try_auto_follow();
        let selected_path = self.selected_filtered_status_path();
        let selected_path_changed = auto_followed || selected_path != previous_path;
        if self.mode == ViewMode::Status {
            if selected_path.is_some() {
                self.refresh_diff(selected_path_changed);
            } else {
                self.clear_diff_state();
            }
        }
        if self
            .status
            .as_deref()
            .is_some_and(|msg| msg.starts_with("git error:"))
        {
            self.status = None;
        }
        if let Some(state) = self.pending_session.take() {
            self.restore_session(&state);
        }
    }

    /// Update `hot_table` with the latest observed mtimes. Entries for
    /// paths missing from the new snapshot are dropped; entries with a
    /// strictly newer mtime are replaced (so a file edited twice within
    /// the hot window re-arms its fade).
    fn merge_hot_table(&mut self, mtimes: HashMap<String, SystemTime>) {
        self.status_view
            .hot_table
            .retain(|p, _| mtimes.contains_key(p));
        for (path, new_mtime) in mtimes {
            self.status_view
                .hot_table
                .entry(path)
                .and_modify(|stored| {
                    if new_mtime > *stored {
                        *stored = new_mtime;
                    }
                })
                .or_insert(new_mtime);
        }
    }

    // ── Terminal pane lifecycle ───────────────────────────────────

    pub fn poll_terminal(&mut self) {
        let events: Vec<BackendEvent> = self
            .terminal
            .backend
            .as_mut()
            .map(|b| b.drain_events())
            .unwrap_or_default();

        for event in events {
            match event {
                BackendEvent::Output { pane, data } => {
                    if let Some(parser) = self.terminal.parsers.get_mut(&pane) {
                        parser.process(&data);
                    }
                }
                BackendEvent::Exited { pane } => {
                    // Single source of truth for pane removal: drain_events no
                    // longer touches the backend's pane map, so we drive the
                    // teardown here. destroy_pane is idempotent against a
                    // pane that close_active_pane already removed.
                    if let Some(backend) = &mut self.terminal.backend {
                        backend.destroy_pane(pane);
                    }
                    self.remove_terminal_pane_state(pane);
                    self.terminal.panes.retain(|p| p.id != pane);
                    self.clamp_active_pane_after_removal();
                }
            }
        }
    }

    pub fn open_new_pane(&mut self) {
        if let Err(e) = self.create_terminal_pane() {
            tracing::error!("create_terminal_pane failed: {e}");
            self.status = Some(format!("terminal error: {e}"));
        }
    }

    pub fn create_terminal_pane(&mut self) -> anyhow::Result<()> {
        let (rows, cols) = self.terminal.size;
        let backend = self
            .terminal
            .backend
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no terminal backend available"))?;

        let id = backend.create_pane(rows.max(1), cols.max(1))?;
        let parser = vt100::Parser::new(rows.max(1), cols.max(1), SCROLLBACK_LINES);
        self.terminal.parsers.insert(id, parser);
        self.terminal.panes.push(PaneInfo {
            id,
            title: "shell".to_string(),
        });
        self.terminal.active = self.terminal.panes.len() - 1;
        tracing::info!(pane = id, "terminal pane opened");
        Ok(())
    }

    pub fn close_active_pane(&mut self) {
        let Some(info) = self.terminal.panes.get(self.terminal.active) else {
            return;
        };
        let id = info.id;
        tracing::info!(pane = id, "terminal pane closed");
        if let Some(backend) = &mut self.terminal.backend {
            backend.destroy_pane(id);
        }
        self.remove_terminal_pane_state(id);
        self.terminal.panes.remove(self.terminal.active);
        self.clamp_active_pane_after_removal();
    }

    fn remove_terminal_pane_state(&mut self, id: PaneId) {
        self.terminal.parsers.remove(&id);
        // Flush any unterminated prompt input so we don't lose the line the
        // user was composing when the pane closes.
        if let Some(buf) = self.terminal.prompt_bufs.remove(&id)
            && !buf.is_empty()
        {
            tracing::info!(target: "prompt", pane = id, text = %buf);
        }
        self.terminal.scroll.remove(&id);
    }

    fn clamp_active_pane_after_removal(&mut self) {
        if self.terminal.panes.is_empty() {
            self.terminal.active = 0;
            self.focus = Focus::DiffViewer;
            self.terminal.fullscreen = false;
        } else {
            self.terminal.active = self.terminal.active.min(self.terminal.panes.len() - 1);
        }
    }

    // ── Repo selection + input bar ────────────────────────────────

    pub fn change_repo(&mut self, new_path: String) {
        // Replacing _stop_tx drops the old sender, signaling the old thread to exit.
        // Replacing the channel drops the old _stop_tx, signaling the old
        // worker to exit at its next recv_timeout boundary.
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
        self.status_view.files.clear();
        self.status_view.selected = 0;
        self.diff.hunks.clear();
        self.diff.scroll = 0;
        self.diff.scroll_x = 0;
        self.status_view.file_scroll_x = 0;
        self.log_view.commits.clear();
        self.log_view.selected = 0;
        self.log_view.diff_title.clear();
        self.log_view.commit_scroll_x = 0;
        self.log_view.reset_drill_down();
        self.status_view.clear_search();
        self.status_view.search_active = false;
        self.status_view.recompute_filter();
        self.diff.search.clear();
        self.status = None;
        self.tracking = None;
        self.focus = Focus::FileList;
        // Drop transient view modes — the previous repo's diff zoom or terminal
        // fullscreen has no meaning under the new working tree.
        self.diff.fullscreen = false;
        self.terminal.fullscreen = false;
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
        self.repo_input.active = false;
        let path = std::mem::take(&mut self.repo_input.buf);
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.status = Some("repo path cannot be empty".to_string());
            return;
        }
        let p = std::path::Path::new(trimmed);
        if !p.is_dir() {
            self.status = Some(format!("not a directory: {trimmed}"));
            return;
        }
        self.change_repo(
            crate::git::resolve_repo_path(p)
                .to_string_lossy()
                .to_string(),
        );
    }

    pub fn repo_input_push(&mut self, ch: char) {
        self.repo_input.buf.push(ch);
    }

    pub fn repo_input_pop(&mut self) {
        self.repo_input.buf.pop();
    }

    pub fn switch_pane(&mut self, idx: usize) {
        if idx < self.terminal.panes.len() {
            self.terminal.active = idx;
            self.focus = Focus::Terminal;
            // Pressing F1..=F9 is a request to interact with a terminal pane;
            // dropping diff fullscreen here keeps focus, render, and hints in
            // sync (otherwise the diff stays zoomed while focus moves away).
            self.diff.fullscreen = false;
        }
    }

    pub fn active_screen(&self) -> Option<&vt100::Screen> {
        let id = self.terminal.active_pane_id()?;
        self.terminal.parsers.get(&id).map(|p| p.screen())
    }

    // ── Diff loading ──────────────────────────────────────────────

    pub fn reload_diff(&mut self) {
        self.refresh_diff(true);
    }

    /// Run `f` with the cached `git2::Repository`, opening it lazily on first
    /// use. Cache is invalidated by `change_repo` so that follow-up calls open
    /// a fresh handle for the new path. Errors from the open propagate so the
    /// caller can surface them in `self.status`.
    fn with_repo<R>(
        &mut self,
        f: impl FnOnce(&git2::Repository) -> anyhow::Result<R>,
    ) -> anyhow::Result<R> {
        if self.repo_cache.is_none() {
            let repo = git2::Repository::discover(self.repo_path.as_str())
                .map_err(|e| anyhow::anyhow!("not a git repository: {e}"))?;
            self.repo_cache = Some(repo);
        }
        // unwrap is sound: we just inserted Some above when None.
        f(self.repo_cache.as_ref().unwrap())
    }

    fn refresh_diff(&mut self, reset_scroll: bool) {
        if self.mode == ViewMode::Log {
            return;
        }
        let previous_scroll = self.diff.scroll;
        let Some(path) = self.selected_filtered_status_path() else {
            self.clear_diff_state();
            return;
        };
        let result = self.with_repo(|repo| load_file_diff(repo, &path));
        if let Err(e) = &result {
            tracing::debug!(error = %e, file = %path, "failed to load diff");
        }
        let mode = if reset_scroll {
            DiffApply::Reset
        } else {
            DiffApply::KeepScroll(previous_scroll)
        };
        self.apply_diff_result(result, mode);
    }

    /// Centralizes the post-load shape used by every diff loader: on success
    /// stash hunks, reset/restore scroll and search cursor, optionally update
    /// the log title, and recompute diff search matches; on error clear state
    /// but preserve the title so the user knows what failed.
    fn apply_diff_result(&mut self, result: anyhow::Result<Vec<DiffHunk>>, mode: DiffApply<'_>) {
        let reset_scroll = matches!(mode, DiffApply::Reset | DiffApply::ResetWithTitle(_));
        match result {
            Ok(hunks) => {
                self.diff.hunks = hunks;
                self.diff.rebuild_lower_cache();
                match mode {
                    DiffApply::Reset | DiffApply::ResetWithTitle(_) => {
                        self.diff.scroll = 0;
                        self.diff.scroll_x = 0;
                        self.diff.search.cursor = 0;
                        self.invalidate_file_view();
                    }
                    DiffApply::KeepScroll(prev) => {
                        // New hunks may be shorter than the prior load, so
                        // clamp against the freshly assigned diff to avoid
                        // leaving an out-of-range scroll that misbehaves on
                        // the next navigation keystroke.
                        self.diff.scroll = prev.min(self.diff.max_scroll());
                    }
                }
                if let DiffApply::ResetWithTitle(title) = mode {
                    self.log_view.diff_title = title.to_string();
                }
                if !self.diff.search.query.is_empty() {
                    self.diff.recompute_matches(reset_scroll);
                }
            }
            Err(_) => {
                self.clear_diff_state();
                if let DiffApply::ResetWithTitle(title) = mode {
                    self.log_view.diff_title = title.to_string();
                }
            }
        }
    }

    fn clear_diff_state(&mut self) {
        self.diff.hunks.clear();
        self.diff.hunks_lines_lower.clear();
        self.diff.line_highlights.clear();
        self.diff.cached_syntax_name = None;
        self.diff.search.matches.clear();
        self.diff.search.cursor = 0;
        self.diff.scroll = 0;
        self.diff.scroll_x = 0;
        self.invalidate_file_view();
    }

    /// Rebuild the lowercase cache from current `hunks`. Call after replacing
    /// `hunks` so per-keystroke search does not re-lowercase line content.
    fn invalidate_file_view(&mut self) {
        self.diff.view = DiffPaneView::Diff;
        self.diff.file_view = FileViewState::default();
    }

    fn current_file_view_key(&self) -> Option<FileViewKey> {
        match self.mode {
            ViewMode::Status => {
                let path = self.selected_filtered_status_file()?.path.clone();
                Some(FileViewKey::Status(path))
            }
            ViewMode::Log => {
                if !self.log_view.drill_down {
                    return None;
                }
                let oid = self.log_view.commits.get(self.log_view.selected)?.oid;
                let path = self
                    .log_view
                    .commit_files
                    .get(self.log_view.file_selected)?
                    .path
                    .clone();
                Some(FileViewKey::Commit { oid, path })
            }
        }
    }

    /// Pick the new-side starting line of the hunk currently visible at the
    /// top of the diff viewport. Walks the flat hunk layout (one header row +
    /// body rows per hunk) and returns the most recent hunk whose header was
    /// reached at or before `self.diff.scroll`. Falls back to the first
    /// parseable hunk when the scroll is past every hunk we could parse.
    fn anchor_for_current_diff(&self) -> Option<usize> {
        let scroll = self.diff.scroll;
        let mut offset = 0usize;
        let mut chosen = None;
        for h in &self.diff.hunks {
            if let Some(n) = parse_hunk_new_start(&h.header) {
                chosen = Some(n);
            }
            offset += 1 + h.lines.len();
            if scroll < offset {
                break;
            }
        }
        chosen
    }

    fn load_file_view(&mut self, key: FileViewKey) {
        let result = match &key {
            FileViewKey::Status(path) => self.with_repo(|repo| load_workdir_file(repo, path)),
            FileViewKey::Commit { oid, path } => {
                let oid = *oid;
                self.with_repo(|repo| load_commit_file_blob(repo, oid, path))
            }
        };
        let anchor = self.anchor_for_current_diff();
        let mut fv = FileViewState {
            key: Some(key),
            anchor_line: anchor,
            ..Default::default()
        };
        match result {
            Ok(content) => {
                fv.scroll = anchor
                    .map(|n| n.saturating_sub(1).saturating_sub(2))
                    .unwrap_or(0);
                fv.total_lines = if content.is_empty() {
                    0
                } else {
                    content.lines().count()
                };
                fv.content = content;
            }
            Err(e) => {
                fv.error = Some(e.to_string());
            }
        }
        self.diff.file_view = fv;
    }

    pub fn toggle_diff_file_view(&mut self) {
        if self.diff.view == DiffPaneView::File {
            self.diff.view = DiffPaneView::Diff;
            return;
        }
        let Some(key) = self.current_file_view_key() else {
            return;
        };
        if self.diff.file_view.key.as_ref() != Some(&key) {
            self.load_file_view(key);
        }
        self.diff.view = DiffPaneView::File;
    }

    // ── Status selection + filter ─────────────────────────────────

    fn restore_selection(&mut self, previous_path: Option<&str>) -> Option<String> {
        if self.status_view.files.is_empty() {
            self.status_view.selected = 0;
            return None;
        }

        if let Some(path) = previous_path
            && let Some(index) = self
                .status_view
                .files
                .iter()
                .position(|file| file.path == path)
        {
            self.status_view.selected = index;
            return Some(path.to_string());
        }

        self.status_view.selected = self
            .status_view
            .selected
            .min(self.status_view.files.len().saturating_sub(1));
        self.status_view
            .files
            .get(self.status_view.selected)
            .map(|file| file.path.clone())
    }

    pub fn filtered_indices(&self) -> &[usize] {
        &self.status_view.filter_cache
    }

    pub fn start_search(&mut self) {
        self.status_view.start_search();
    }

    pub fn cancel_search(&mut self) {
        self.status_view.cancel_search();
        self.refresh_status_diff_after_filter_change();
    }

    pub fn confirm_search(&mut self) {
        if self.status_view.confirm_search() {
            self.refresh_status_diff_after_filter_change();
        }
    }

    pub fn search_push(&mut self, ch: char) {
        self.status_view.search_push(ch);
        self.refresh_status_diff_after_filter_change();
    }

    pub fn search_pop(&mut self) {
        self.status_view.search_pop();
        self.refresh_status_diff_after_filter_change();
    }

    pub fn file_scroll_left(&mut self) {
        let target = self.upper_scroll_x_mut();
        *target = target.saturating_sub(4);
    }

    pub fn file_scroll_right(&mut self) {
        let max = self.upper_scroll_x_max();
        let target = self.upper_scroll_x_mut();
        *target = target.saturating_add(4).min(max);
    }

    fn upper_scroll_x_mut(&mut self) -> &mut usize {
        match self.mode {
            ViewMode::Status => &mut self.status_view.file_scroll_x,
            ViewMode::Log if self.log_view.drill_down => &mut self.log_view.file_scroll_x,
            ViewMode::Log => &mut self.log_view.commit_scroll_x,
        }
    }

    fn upper_scroll_x_max(&self) -> usize {
        // Cap at the longest visible entry's char width so we don't drift past
        // the last column of any rendered row. Each branch consults a
        // length-keyed `Cell` cache so repeated keystrokes don't re-walk the
        // full list (and re-count chars per item) every press.
        fn cached_max<'a, T: 'a>(
            cache: &Cell<Option<(usize, usize)>>,
            items: &'a [T],
            width_of: impl Fn(&'a T) -> usize,
        ) -> usize {
            let len = items.len();
            if let Some((cached_len, cached_max)) = cache.get()
                && cached_len == len
            {
                return cached_max;
            }
            let max = items.iter().map(width_of).max().unwrap_or(0);
            cache.set(Some((len, max)));
            max
        }
        match self.mode {
            ViewMode::Status => cached_max(
                &self.status_view.path_width_cache,
                &self.status_view.files,
                |f| f.path.chars().count(),
            ),
            ViewMode::Log if self.log_view.drill_down => cached_max(
                &self.log_view.commit_files_width_cache,
                &self.log_view.commit_files,
                |f| f.path.chars().count(),
            ),
            ViewMode::Log => cached_max(
                &self.log_view.commit_width_cache,
                &self.log_view.commits,
                |c| c.summary.chars().count(),
            ),
        }
    }

    fn selected_filtered_status_path(&self) -> Option<String> {
        self.selected_filtered_status_file().map(|f| f.path.clone())
    }

    /// Borrow-only counterpart of `selected_filtered_status_path` so callers
    /// that just need to read the path don't pay for an allocation. Uses
    /// `binary_search` since `filter_cache` is built in ascending order by
    /// `recompute_filter`.
    pub fn selected_filtered_status_file(&self) -> Option<&ChangedFile> {
        if self
            .filtered_indices()
            .binary_search(&self.status_view.selected)
            .is_err()
        {
            return None;
        }
        self.status_view.files.get(self.status_view.selected)
    }

    fn sync_selection_to_filter(&mut self) -> bool {
        let target = {
            let indices = self.filtered_indices();
            if indices.is_empty() {
                return false;
            }
            if indices.contains(&self.status_view.selected) {
                self.status_view.selected
            } else {
                indices[0]
            }
        };

        if target == self.status_view.selected {
            false
        } else {
            self.status_view.selected = target;
            true
        }
    }

    fn refresh_status_diff_after_filter_change(&mut self) {
        let selection_changed = self.sync_selection_to_filter();
        if self.selected_filtered_status_path().is_none() {
            self.clear_diff_state();
        } else if selection_changed || self.diff.hunks.is_empty() {
            self.reload_diff();
        }
    }

    /// Dispatches a navigation action to the appropriate log list (commit or file).
    /// Returns `true` if the action was handled (i.e. we are in Log mode).
    fn navigate_log_list(&mut self, commit_nav: fn(&mut Self), file_nav: fn(&mut Self)) -> bool {
        if self.mode != ViewMode::Log {
            return false;
        }
        if self.log_view.drill_down {
            file_nav(self);
        } else {
            commit_nav(self);
        }
        true
    }

    /// Move `selected` by `delta` positions within the active filter view.
    /// Handles both empty-query (full file list) and non-empty (filtered subset)
    /// cases uniformly.
    fn move_selected_in_filter(&mut self, delta: isize) {
        // Resolve the new selection in a scoped block so the borrow on
        // filtered_indices does not outlive the mutating reload below.
        let resolved = {
            let indices = self.filtered_indices();
            if indices.is_empty() {
                None
            } else {
                let pos = indices.iter().position(|&i| i == self.status_view.selected);
                let new_pos = match pos {
                    Some(p) => {
                        let last = indices.len() as isize - 1;
                        (p as isize + delta).clamp(0, last) as usize
                    }
                    None => 0,
                };
                Some((pos, new_pos, indices[new_pos]))
            }
        };
        if let Some((pos, new_pos, new_selected)) = resolved
            && (Some(new_pos) != pos || self.status_view.selected != new_selected)
        {
            self.status_view.selected = new_selected;
            self.status_view.file_scroll_x = 0;
            self.reload_diff();
        }
    }

    // ── Selection navigation (status + log shared) ────────────────

    pub fn select_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                self.mark_user_navigated();
                if self.navigate_log_list(Self::log_select_up, Self::log_file_select_up) {
                    return;
                }
                self.move_selected_in_filter(-1);
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_up(1);
                } else {
                    self.diff.scroll = self.diff.scroll.saturating_sub(1);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn select_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                self.mark_user_navigated();
                if self.navigate_log_list(Self::log_select_down, Self::log_file_select_down) {
                    return;
                }
                self.move_selected_in_filter(1);
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_down(1);
                } else {
                    self.diff.scroll = self
                        .diff
                        .scroll
                        .saturating_add(1)
                        .min(self.diff.max_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_up(&mut self) {
        match self.focus {
            Focus::FileList => {
                self.mark_user_navigated();
                if self.navigate_log_list(Self::log_page_up, Self::log_file_page_up) {
                    return;
                }
                self.move_selected_in_filter(-(LIST_PAGE_SIZE as isize));
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_up(DIFF_PAGE_SIZE);
                } else {
                    self.diff.scroll = self.diff.scroll.saturating_sub(DIFF_PAGE_SIZE);
                }
            }
            Focus::Terminal => {}
        }
    }

    pub fn page_down(&mut self) {
        match self.focus {
            Focus::FileList => {
                self.mark_user_navigated();
                if self.navigate_log_list(Self::log_page_down, Self::log_file_page_down) {
                    return;
                }
                self.move_selected_in_filter(LIST_PAGE_SIZE as isize);
            }
            Focus::DiffViewer => {
                if self.diff.view == DiffPaneView::File {
                    self.diff.file_view.scroll_down(DIFF_PAGE_SIZE);
                } else {
                    self.diff.scroll = self
                        .diff
                        .scroll
                        .saturating_add(DIFF_PAGE_SIZE)
                        .min(self.diff.max_scroll());
                }
            }
            Focus::Terminal => {}
        }
    }

    /// Record that the user just moved selection so auto-follow holds off
    /// for a short grace period. Also clears the "we steered to this path"
    /// memory — the user has taken back control.
    fn mark_user_navigated(&mut self) {
        self.last_manual_nav_at = Some(Instant::now());
        self.auto_followed_path = None;
    }

    /// Decide whether the file list should auto-follow to a new hot file,
    /// and perform the move if so. Returns `true` when selection changed.
    /// Caller is responsible for refreshing the diff afterward.
    fn try_auto_follow(&mut self) -> bool {
        if !self.cfg_agent_indicator.enabled || !self.cfg_agent_indicator.auto_follow {
            return false;
        }
        if self.focus != Focus::FileList || self.mode != ViewMode::Status {
            return false;
        }
        let idle = match self.last_manual_nav_at {
            None => true,
            Some(t) => t.elapsed() >= Duration::from_secs(2),
        };
        if !idle {
            return false;
        }
        let Some(target_path) = self.freshest_hot_path() else {
            return false;
        };
        let current_path = self.selected_filtered_status_path();
        if current_path.as_deref() == Some(target_path.as_str()) {
            return false;
        }
        if self.auto_followed_path.as_deref() == Some(target_path.as_str()) {
            return false;
        }
        let moved = self.select_status_file_by_path(&target_path);
        if moved {
            self.auto_followed_path = Some(target_path);
        }
        moved
    }

    /// Path with the newest mtime among files that are still inside the
    /// configured hot window and pass the current filter. Returns `None`
    /// when no qualifying file exists. Tiebreak by path for stability.
    fn freshest_hot_path(&self) -> Option<String> {
        if self.status_view.hot_table.is_empty() {
            return None;
        }
        let now = SystemTime::now();
        let window = Duration::from_secs(self.cfg_agent_indicator.hot_window_secs);
        // Walk the filtered index list and probe `hot_table` by path. The
        // previous implementation built a per-tick `HashSet` of filtered
        // paths inside `try_auto_follow`'s hot loop, which allocated every
        // snapshot tick. Tiebreak by smaller path for stability.
        let mut best: Option<(&str, SystemTime)> = None;
        for &idx in self.filtered_indices() {
            let Some(file) = self.status_view.files.get(idx) else {
                continue;
            };
            let Some(&mtime) = self.status_view.hot_table.get(&file.path) else {
                continue;
            };
            let in_window = now
                .duration_since(mtime)
                .map(|d| d <= window)
                .unwrap_or(true);
            if !in_window {
                continue;
            }
            let replace = match best {
                None => true,
                Some((bp, bm)) => mtime > bm || (mtime == bm && file.path.as_str() < bp),
            };
            if replace {
                best = Some((file.path.as_str(), mtime));
            }
        }
        best.map(|(p, _)| p.to_string())
    }

    /// Move the selection cursor to `path` if it exists in the unfiltered
    /// status list. Returns whether selection actually changed.
    fn select_status_file_by_path(&mut self, path: &str) -> bool {
        if let Some(idx) = self.status_view.files.iter().position(|f| f.path == path)
            && self.status_view.selected != idx
        {
            self.status_view.selected = idx;
            self.status_view.file_scroll_x = 0;
            return true;
        }
        false
    }

    // ── Log view ──────────────────────────────────────────────────

    fn load_commit_diff_for_selected(&mut self) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => {
                self.clear_diff_state();
                self.log_view.diff_title.clear();
                return;
            }
        };
        let result = self.with_repo(|repo| load_commit_diff(repo, oid));
        if let Err(e) = &result {
            tracing::debug!(error = %e, "failed to load commit diff");
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }

    pub fn log_drill_in(&mut self) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => return,
        };
        match self.with_repo(|repo| load_commit_files(repo, oid)) {
            Ok(files) => {
                self.log_view.commit_files = files;
                self.log_view.file_selected = 0;
                self.log_view.drill_down = true;
                if self.log_view.commit_files.is_empty() {
                    self.clear_diff_state();
                    self.log_view.diff_title = title;
                } else {
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "failed to load commit files");
            }
        }
    }

    pub fn log_drill_out(&mut self) {
        self.log_view.reset_drill_down();
        self.load_commit_diff_for_selected();
    }

    pub fn log_file_select_up(&mut self) {
        if self.log_view.file_select_up(1) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_select_down(&mut self) {
        if self.log_view.file_select_down(1) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_up(&mut self) {
        if self.log_view.file_select_up(LIST_PAGE_SIZE) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    pub fn log_file_page_down(&mut self) {
        if self.log_view.file_select_down(LIST_PAGE_SIZE) {
            self.load_file_diff_for_log_file_selected();
        }
    }

    fn load_file_diff_for_log_file_selected(&mut self) {
        let Some((oid, short_id, commit_title)) = self
            .log_view
            .commits
            .get(self.log_view.selected)
            .map(|c| (c.oid, c.short_id.clone(), c.to_string()))
        else {
            self.clear_diff_state();
            self.log_view.diff_title.clear();
            return;
        };
        let Some(path) = self
            .log_view
            .commit_files
            .get(self.log_view.file_selected)
            .map(|f| f.path.clone())
        else {
            self.clear_diff_state();
            self.log_view.diff_title = commit_title;
            return;
        };
        let title = format!("{short_id} {path}");
        let result = self.with_repo(|repo| load_commit_file_diff(repo, oid, &path));
        if let Err(e) = &result {
            tracing::debug!(error = %e, file = %path, "failed to load commit file diff");
        }
        self.apply_diff_result(result, DiffApply::ResetWithTitle(&title));
    }

    // ── Mode, theme, focus, fullscreen ────────────────────────────

    pub fn toggle_mode(&mut self) {
        self.clear_diff_state();
        match self.mode {
            ViewMode::Status => {
                self.mode = ViewMode::Log;
                self.log_view.reset_drill_down();
                self.log_view.commit_scroll_x = 0;
                match self.with_repo(|repo| load_commit_log(repo, COMMIT_LOG_LIMIT)) {
                    Ok(commits) => {
                        self.log_view.commits = commits;
                        self.log_view.selected = 0;
                        self.load_commit_diff_for_selected();
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to load commit log");
                        self.log_view.commits.clear();
                        self.log_view.selected = 0;
                        self.status = Some(format!("git error: {e}"));
                    }
                }
            }
            ViewMode::Log => {
                self.mode = ViewMode::Status;
                self.log_view.reset_drill_down();
                self.refresh_diff(true);
            }
        }
    }

    pub fn log_select_up(&mut self) {
        if cursor_up(&mut self.log_view.selected, 1) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_select_down(&mut self) {
        if cursor_down(&mut self.log_view.selected, self.log_view.commits.len(), 1) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_up(&mut self) {
        if cursor_up(&mut self.log_view.selected, LIST_PAGE_SIZE) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn log_page_down(&mut self) {
        if cursor_down(
            &mut self.log_view.selected,
            self.log_view.commits.len(),
            LIST_PAGE_SIZE,
        ) {
            self.log_view.commit_scroll_x = 0;
            self.load_commit_diff_for_selected();
        }
    }

    pub fn set_accent_index(&mut self, idx: usize) {
        // Normalize on entry so we never persist out-of-range indices to the
        // session file, even though `current_accent` would tolerate them.
        self.accent_idx = idx % crate::config::Accent::ALL.len();
    }

    pub fn cycle_accent(&mut self) {
        self.accent_idx = (self.accent_idx + 1) % crate::config::Accent::ALL.len();
    }

    pub fn current_accent(&self) -> ratatui::style::Color {
        crate::config::Accent::from_index(self.accent_idx).color()
    }

    pub fn cycle_focus_forward(&mut self) {
        if self.diff.fullscreen {
            return;
        }
        if self.terminal.fullscreen {
            let len = self.terminal.panes.len();
            if len > 0 {
                self.terminal.active = (self.terminal.active + 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                self.focus = Focus::DiffViewer;
            }
            Focus::DiffViewer => {
                if !self.terminal.panes.is_empty() {
                    self.terminal.active = 0;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::FileList;
                }
            }
            Focus::Terminal => {
                if self.terminal.active + 1 < self.terminal.panes.len() {
                    self.terminal.active += 1;
                } else {
                    self.focus = Focus::FileList;
                }
            }
        }
    }

    pub fn cycle_focus_backward(&mut self) {
        if self.diff.fullscreen {
            return;
        }
        if self.terminal.fullscreen {
            let len = self.terminal.panes.len();
            if len > 0 {
                self.terminal.active = (self.terminal.active + len - 1) % len;
            }
            return;
        }
        match self.focus {
            Focus::FileList => {
                if !self.terminal.panes.is_empty() {
                    self.terminal.active = self.terminal.panes.len() - 1;
                    self.focus = Focus::Terminal;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
            Focus::DiffViewer => {
                self.focus = Focus::FileList;
            }
            Focus::Terminal => {
                if self.terminal.active > 0 {
                    self.terminal.active -= 1;
                } else {
                    self.focus = Focus::DiffViewer;
                }
            }
        }
    }

    pub fn toggle_terminal_fullscreen(&mut self) {
        if !self.terminal.fullscreen && self.terminal.panes.is_empty() {
            return;
        }
        self.terminal.fullscreen = !self.terminal.fullscreen;
        if self.terminal.fullscreen {
            self.focus = Focus::Terminal;
            self.diff.fullscreen = false;
        }
    }

    pub fn toggle_diff_fullscreen(&mut self) {
        self.diff.fullscreen = !self.diff.fullscreen;
        if self.diff.fullscreen {
            self.focus = Focus::DiffViewer;
            self.terminal.fullscreen = false;
        }
    }

    // ── Session save / restore ────────────────────────────────────

    pub fn set_pending_session(&mut self, state: crate::session::SessionState) {
        self.pending_session = Some(state);
    }

    pub fn save_session(&self) -> crate::session::SessionState {
        crate::session::SessionState {
            focus: Some(self.focus),
            selected_file: self
                .status_view
                .files
                .get(self.status_view.selected)
                .map(|f| f.path.clone()),
            scroll: self.diff.scroll,
            active_pane: self.terminal.active,
            terminal_fullscreen: self.terminal.fullscreen,
            diff_fullscreen: self.diff.fullscreen,
            mode: Some(self.mode),
            log_selected: self.log_view.selected,
            accent_idx: self.accent_idx,
            log_drill_down: self.log_view.drill_down,
            log_file_selected: self.log_view.file_selected,
        }
    }

    pub fn restore_session(&mut self, state: &crate::session::SessionState) {
        // Pane / focus / fullscreen restoration — independent of view mode.
        self.terminal.active = state
            .active_pane
            .min(self.terminal.panes.len().saturating_sub(1));
        if let Some(focus) = state.focus {
            if focus == Focus::Terminal && self.terminal.panes.is_empty() {
                self.focus = Focus::FileList;
            } else {
                self.focus = focus;
            }
        }
        self.terminal.fullscreen = state.terminal_fullscreen && !self.terminal.panes.is_empty();
        if self.terminal.fullscreen {
            self.focus = Focus::Terminal;
        }
        self.diff.fullscreen = state.diff_fullscreen && !self.terminal.fullscreen;
        if self.diff.fullscreen {
            self.focus = Focus::DiffViewer;
        }
        self.set_accent_index(state.accent_idx);

        // Mode-specific diff/scroll restoration. We avoid loading a workdir diff
        // when the saved mode is Log — otherwise we'd waste a load and clamp the
        // scroll against the wrong diff length.
        match state.mode {
            Some(ViewMode::Log) => self.restore_log_session(state),
            _ => self.restore_status_session(state),
        }

        tracing::debug!(
            focus = ?state.focus,
            file = ?state.selected_file,
            scroll = state.scroll,
            mode = ?state.mode,
            drill = state.log_drill_down,
            "session restored"
        );
    }

    fn restore_status_session(&mut self, state: &crate::session::SessionState) {
        if let Some(path) = &state.selected_file
            && let Some(idx) = self.status_view.files.iter().position(|f| &f.path == path)
        {
            self.status_view.selected = idx;
            self.refresh_diff(true);
            self.diff.scroll = state.scroll.min(self.diff.max_scroll());
        }
        // If the saved file is no longer present, leave selected/scroll as they
        // were after the initial snapshot — applying saved_scroll to a different
        // file would jump the user to an unrelated location.
    }

    fn restore_log_session(&mut self, state: &crate::session::SessionState) {
        let commits = match self.with_repo(|repo| load_commit_log(repo, COMMIT_LOG_LIMIT)) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to restore commit log");
                return;
            }
        };
        self.log_view.commits = commits;
        self.log_view.selected = state
            .log_selected
            .min(self.log_view.commits.len().saturating_sub(1));
        self.mode = ViewMode::Log;

        if state.log_drill_down {
            self.restore_log_drill_down(state);
        } else {
            self.load_commit_diff_for_selected();
        }
        self.diff.scroll = state.scroll.min(self.diff.max_scroll());
    }

    fn restore_log_drill_down(&mut self, state: &crate::session::SessionState) {
        let (oid, title) = match self.log_view.commits.get(self.log_view.selected) {
            Some(entry) => (entry.oid, entry.to_string()),
            None => {
                self.load_commit_diff_for_selected();
                return;
            }
        };
        match self.with_repo(|repo| load_commit_files(repo, oid)) {
            Ok(files) => {
                self.log_view.commit_files = files;
                self.log_view.drill_down = true;
                if self.log_view.commit_files.is_empty() {
                    self.log_view.file_selected = 0;
                    self.clear_diff_state();
                    self.log_view.diff_title = title;
                } else {
                    self.log_view.file_selected = state
                        .log_file_selected
                        .min(self.log_view.commit_files.len().saturating_sub(1));
                    self.load_file_diff_for_log_file_selected();
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, "failed to load drill-down commit files");
                self.load_commit_diff_for_selected();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::{
        ChangeStatus, CommitEntry, DiffHunk, DiffLine, LineKind, load_commit_log,
    };
    use crate::test_util::{make_repo, open_repo, run_git};
    use std::path::Path;

    /// Build an inert SnapshotChannel for tests: real receiver, real stop
    /// sender, but no worker thread driving the receiver.
    ///
    /// Drops `_stop_rx` immediately on purpose: the only contract of `_stop_tx`
    /// is "dropped → worker observes disconnect". Since there is no worker
    /// here, nothing waits on either side, and dropping `_stop_rx` upfront
    /// keeps the helper's tuple shape minimal. If a future test ever spawns
    /// a real worker against this channel, it must keep `_stop_rx` alive.
    fn dummy_snapshot_channel() -> (SnapshotChannel, std::sync::mpsc::Sender<SnapshotMsg>) {
        let (tx, rx) = mpsc::channel::<SnapshotMsg>();
        let (stop_tx, _stop_rx) = mpsc::sync_channel::<()>(0);
        (
            SnapshotChannel {
                rx,
                _stop_tx: stop_tx,
            },
            tx,
        )
    }

    fn app_with_files(files: Vec<&str>) -> App {
        let (snapshot, _tx) = dummy_snapshot_channel();
        let mut status_view = StatusView {
            files: files
                .into_iter()
                .map(|path| ChangedFile::new(path.to_string(), ChangeStatus::Modified))
                .collect(),
            ..Default::default()
        };
        status_view.recompute_filter();
        App {
            mode: ViewMode::Status,
            status_view,
            diff: DiffPane::default(),
            focus: Focus::FileList,
            status: None,
            repo_path: ".".to_string(),
            log_view: LogView::default(),
            terminal: TerminalState::new(None, false),
            repo_input: RepoInput::default(),
            accent_idx: 0,
            tracking: None,
            snapshot,
            pending_session: None,
            repo_cache: None,
            cfg_agent_indicator: crate::config::AgentIndicatorConfig::default(),
            last_manual_nav_at: None,
            auto_followed_path: None,
        }
    }

    fn context_hunk(lines: &[&str]) -> DiffHunk {
        DiffHunk {
            header: "@@ -1 +1 @@".to_string(),
            lines: lines
                .iter()
                .map(|content| DiffLine {
                    kind: LineKind::Context,
                    content: (*content).to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn selection_clamps_when_file_list_shrinks() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.status_view.selected = 2;
        app.status_view.files = vec![ChangedFile::new("a.rs".to_string(), ChangeStatus::Modified)];

        let selected_path = app.restore_selection(Some("c.rs"));

        assert_eq!(selected_path.as_deref(), Some("a.rs"));
        assert_eq!(app.status_view.selected, 0);
    }

    #[test]
    fn selection_prefers_same_path_after_refresh() {
        let mut app = app_with_files(vec!["a.rs", "b.rs", "c.rs"]);
        app.status_view.selected = 1;
        app.status_view.files = vec![
            ChangedFile::new("a.rs".to_string(), ChangeStatus::Modified),
            ChangedFile::new("c.rs".to_string(), ChangeStatus::Modified),
            ChangedFile::new("b.rs".to_string(), ChangeStatus::Modified),
        ];

        let selected_path = app.restore_selection(Some("b.rs"));

        assert_eq!(selected_path.as_deref(), Some("b.rs"));
        assert_eq!(app.status_view.selected, 2);
    }

    #[test]
    fn diff_scroll_saturates_on_page_up() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.scroll = 3;

        app.page_up();

        assert_eq!(app.diff.scroll, 0);
    }

    #[test]
    fn diff_scroll_clamps_at_last_line_on_select_down() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        // 1 hunk = header + 1 content line = 2 total lines, max_scroll = 1
        app.diff.hunks = vec![context_hunk(&["x"])];
        app.diff.scroll = 1; // already at max

        app.select_down();

        assert_eq!(app.diff.scroll, 1, "scroll must not exceed last line index");
    }

    #[test]
    fn diff_scroll_clamps_at_last_line_on_page_down() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.hunks = vec![context_hunk(&["x"])];
        app.diff.scroll = 0;

        app.page_down(); // +20, but max is 1

        assert_eq!(app.diff.scroll, 1);
    }

    #[test]
    fn diff_scroll_handles_large_restored_offset() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;
        app.diff.hunks = vec![context_hunk(&["x"])];
        app.diff.scroll = usize::MAX;

        app.select_down();

        assert_eq!(app.diff.scroll, 1);
    }

    #[test]
    fn diff_match_refresh_can_preserve_manual_scroll() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["needle"])];
        app.diff.search.query = "needle".to_string();
        app.diff.search.query_lower = "needle".to_string();
        app.diff.scroll = 7;

        app.diff.recompute_matches(false);

        assert_eq!(app.diff.search.matches, vec![1]);
        assert_eq!(app.diff.scroll, 7);
    }

    #[test]
    fn diff_search_input_scrolls_to_first_match() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["alpha", "needle"])];

        app.diff.search_push('n');

        assert_eq!(app.diff.search.matches, vec![2]);
        assert_eq!(app.diff.scroll, 2);
    }

    #[test]
    fn status_search_with_no_matches_clears_stale_diff() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["stale"])];

        app.search_push('z');

        assert!(app.filtered_indices().is_empty());
        assert!(app.diff.hunks.is_empty());
    }

    #[test]
    fn terminal_scrollback_uses_full_buffer() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.active = 0;
        app.terminal.size = (3, 10);

        let mut parser = vt100::Parser::new(3, 10, SCROLLBACK_LINES);
        parser.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n");
        app.terminal.parsers.insert(1, parser);
        // Request scrolling well past screen height; vt100 supports
        // arbitrary offsets up to the buffered line count.
        app.terminal.scroll.insert(1, 6);

        app.terminal.sync_scroll();

        let actual = app.terminal.parsers.get(&1).unwrap().screen().scrollback();
        assert_eq!(actual, 6);
        assert_eq!(app.terminal.scroll.get(&1).copied(), Some(6));
    }

    #[test]
    fn terminal_scrollback_clamps_to_buffered_rows() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.active = 0;
        app.terminal.size = (3, 10);

        let mut parser = vt100::Parser::new(3, 10, SCROLLBACK_LINES);
        // Only a handful of buffered rows exist; an outsized request must
        // clamp to whatever vt100 actually has, never panic.
        parser.process(b"1\r\n2\r\n3\r\n4\r\n5\r\n");
        app.terminal.parsers.insert(1, parser);
        app.terminal.scroll.insert(1, 999);

        app.terminal.sync_scroll();

        let stored = app.terminal.scroll.get(&1).copied().unwrap_or(0);
        let actual = app.terminal.parsers.get(&1).unwrap().screen().scrollback();
        assert_eq!(stored, actual);
        assert!(actual < 999);
    }

    #[test]
    fn switch_pane_moves_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![
            PaneInfo {
                id: 1,
                title: "shell 1".into(),
            },
            PaneInfo {
                id: 2,
                title: "shell 2".into(),
            },
        ];
        assert_eq!(app.focus, Focus::FileList);
        app.switch_pane(1);
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.terminal.active, 1);
    }

    #[test]
    fn switch_pane_ignores_out_of_range() {
        let mut app = app_with_files(vec![]);
        app.switch_pane(5);
        assert_eq!(app.terminal.active, 0);
    }

    #[test]
    fn switch_pane_exits_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_diff_fullscreen();
        assert!(app.diff.fullscreen);

        app.switch_pane(0);

        assert!(!app.diff.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
        assert_eq!(app.terminal.active, 0);
    }

    #[test]
    fn toggle_fullscreen_switches_focus_to_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        assert_eq!(app.focus, Focus::FileList);

        app.toggle_terminal_fullscreen();

        assert!(app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn toggle_fullscreen_noop_with_no_panes() {
        let mut app = app_with_files(vec![]);
        assert!(app.terminal.panes.is_empty());

        app.toggle_terminal_fullscreen();

        assert!(!app.terminal.fullscreen);
    }

    #[test]
    fn toggle_diff_fullscreen_sets_flag_and_focuses_diff_viewer() {
        let mut app = app_with_files(vec![]);
        assert_eq!(app.focus, Focus::FileList);

        app.toggle_diff_fullscreen();

        assert!(app.diff.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);

        app.toggle_diff_fullscreen();

        assert!(!app.diff.fullscreen);
        // Exiting zoom leaves focus on DiffViewer (no reason to bounce back).
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn toggle_diff_fullscreen_exits_terminal_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_terminal_fullscreen();
        assert!(app.terminal.fullscreen);

        app.toggle_diff_fullscreen();

        assert!(app.diff.fullscreen);
        assert!(!app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn toggle_terminal_fullscreen_exits_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_diff_fullscreen();
        assert!(app.diff.fullscreen);

        app.toggle_terminal_fullscreen();

        assert!(app.terminal.fullscreen);
        assert!(!app.diff.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn cycle_focus_is_noop_in_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.toggle_diff_fullscreen();
        assert_eq!(app.focus, Focus::DiffViewer);

        app.cycle_focus_forward();
        assert_eq!(app.focus, Focus::DiffViewer);

        app.cycle_focus_backward();
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn close_last_pane_exits_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];
        app.terminal.fullscreen = true;
        app.focus = Focus::Terminal;
        app.terminal.scroll.insert(1, 3);
        app.terminal.prompt_bufs.insert(1, "cargo test".to_string());
        app.terminal.parsers.insert(1, vt100::Parser::new(3, 10, 0));

        app.close_active_pane();

        assert!(!app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
        assert!(!app.terminal.scroll.contains_key(&1));
        assert!(!app.terminal.prompt_bufs.contains_key(&1));
        assert!(!app.terminal.parsers.contains_key(&1));
    }

    #[test]
    fn restore_session_restores_active_pane_even_when_focus_is_not_terminal() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![
            PaneInfo {
                id: 1,
                title: "shell 1".into(),
            },
            PaneInfo {
                id: 2,
                title: "shell 2".into(),
            },
        ];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            active_pane: 1,
            ..Default::default()
        });

        assert_eq!(app.focus, Focus::FileList);
        assert_eq!(app.terminal.active, 1);
    }

    #[test]
    fn restore_session_fullscreen_forces_terminal_focus() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            terminal_fullscreen: true,
            ..Default::default()
        });

        assert!(app.terminal.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn restore_session_diff_fullscreen_forces_diff_focus() {
        let mut app = app_with_files(vec![]);

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            diff_fullscreen: true,
            ..Default::default()
        });

        assert!(app.diff.fullscreen);
        assert_eq!(app.focus, Focus::DiffViewer);
    }

    #[test]
    fn restore_session_prefers_terminal_fullscreen_over_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.terminal.panes = vec![PaneInfo {
            id: 1,
            title: "shell".into(),
        }];

        app.restore_session(&crate::session::SessionState {
            focus: Some(Focus::FileList),
            terminal_fullscreen: true,
            diff_fullscreen: true,
            ..Default::default()
        });

        assert!(app.terminal.fullscreen);
        assert!(!app.diff.fullscreen);
        assert_eq!(app.focus, Focus::Terminal);
    }

    #[test]
    fn save_session_round_trips_diff_fullscreen() {
        let mut app = app_with_files(vec![]);
        app.toggle_diff_fullscreen();
        assert!(app.diff.fullscreen);

        let state = app.save_session();
        assert!(state.diff_fullscreen);

        let mut other = app_with_files(vec![]);
        other.restore_session(&state);
        assert!(other.diff.fullscreen);
        assert_eq!(other.focus, Focus::DiffViewer);
    }

    #[test]
    fn restore_session_normalizes_accent_index() {
        let mut app = app_with_files(vec![]);

        app.restore_session(&crate::session::SessionState {
            accent_idx: usize::MAX,
            ..Default::default()
        });

        assert_eq!(
            app.accent_idx,
            usize::MAX % crate::config::Accent::ALL.len()
        );
    }

    #[test]
    fn restore_session_keeps_log_scroll_after_loading_commit_diff() {
        let (_dir, path) = make_repo();
        let file_path = Path::new(&path).join("a.rs");
        std::fs::write(
            &file_path,
            "fn main() {\n    println!(\"one\");\n    println!(\"two\");\n}\n",
        )
        .unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);

        let mut app = app_with_files(vec![]);
        app.repo_path = path;

        app.restore_session(&crate::session::SessionState {
            mode: Some(ViewMode::Log),
            scroll: 2,
            ..Default::default()
        });

        assert_eq!(app.mode, ViewMode::Log);
        assert!(!app.diff.hunks.is_empty());
        assert_eq!(app.diff.scroll, 2);
    }

    #[test]
    fn log_drill_in_clears_stale_diff_for_empty_commit() {
        let (_dir, path) = make_repo();
        run_git(&path, &["commit", "--allow-empty", "-m", "empty"]);

        let mut app = app_with_files(vec![]);
        app.repo_path = path.clone();
        app.mode = ViewMode::Log;
        app.log_view.commits = load_commit_log(&open_repo(&path), 1).unwrap();
        app.diff.hunks = vec![context_hunk(&["stale"])];
        app.log_view.diff_title = "stale".to_string();

        app.log_drill_in();

        assert!(app.log_view.drill_down);
        assert!(app.log_view.commit_files.is_empty());
        assert!(app.diff.hunks.is_empty());
        assert!(app.log_view.diff_title.contains("empty"));
    }

    #[test]
    fn successful_snapshot_preserves_terminal_status() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            status: Some("terminal error: backend unavailable".to_string()),
            snapshot,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: Vec::new(),
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(
            app.status.as_deref(),
            Some("terminal error: backend unavailable")
        );
    }

    #[test]
    fn successful_snapshot_clears_git_status() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            status: Some("git error: not a repo".to_string()),
            snapshot,
            ..app_with_files(vec![])
        };

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: Vec::new(),
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(app.status, None);
    }

    #[test]
    fn snapshot_refresh_clamps_selection_to_active_filter() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            snapshot,
            ..app_with_files(vec!["bar.rs"])
        };
        app.status_view.search_query = "bar".to_string();
        app.status_view.search_query_lower = "bar".to_string();
        app.status_view.recompute_filter();

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: vec![
                    ChangedFile::new("aaa.rs".to_string(), ChangeStatus::Modified),
                    ChangedFile::new("bar2.rs".to_string(), ChangeStatus::Modified),
                ],
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert_eq!(app.filtered_indices(), &[1]);
        assert_eq!(app.status_view.selected, 1);
        assert_eq!(
            app.status_view.files[app.status_view.selected].path,
            "bar2.rs"
        );
    }

    #[test]
    fn snapshot_refresh_with_no_filter_matches_clears_stale_diff() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            snapshot,
            ..app_with_files(vec!["bar.rs"])
        };
        app.status_view.search_query = "bar".to_string();
        app.status_view.search_query_lower = "bar".to_string();
        app.status_view.recompute_filter();
        app.diff.hunks = vec![context_hunk(&["stale"])];

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: vec![ChangedFile::new(
                    "aaa.rs".to_string(),
                    ChangeStatus::Modified,
                )],
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        assert!(app.filtered_indices().is_empty());
        assert!(app.diff.hunks.is_empty());
    }

    #[test]
    fn move_selected_in_filter_resets_horizontal_scroll() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.file_scroll_x = 12;
        app.move_selected_in_filter(1);
        assert_eq!(app.status_view.selected, 1);
        assert_eq!(app.status_view.file_scroll_x, 0);
    }

    #[test]
    fn log_select_down_resets_commit_scroll() {
        let mut app = app_with_files(vec![]);
        app.mode = ViewMode::Log;
        app.log_view.commits = vec![
            CommitEntry {
                oid: git2::Oid::zero(),
                short_id: "0000000".into(),
                summary: "first".into(),
                author: "T".into(),
                time: 0,
            },
            CommitEntry {
                oid: git2::Oid::zero(),
                short_id: "1111111".into(),
                summary: "second".into(),
                author: "T".into(),
                time: 0,
            },
        ];
        app.log_view.commit_scroll_x = 9;
        app.log_select_down();
        assert_eq!(app.log_view.selected, 1);
        assert_eq!(app.log_view.commit_scroll_x, 0);
    }

    #[test]
    fn log_file_select_down_resets_file_scroll() {
        let mut app = app_with_files(vec![]);
        app.mode = ViewMode::Log;
        app.log_view.drill_down = true;
        app.log_view.commits = vec![CommitEntry {
            oid: git2::Oid::zero(),
            short_id: "0000000".into(),
            summary: "first".into(),
            author: "T".into(),
            time: 0,
        }];
        app.log_view.commit_files = vec![
            ChangedFile::new("x.rs".into(), ChangeStatus::Modified),
            ChangedFile::new("y.rs".into(), ChangeStatus::Modified),
        ];
        app.log_view.file_scroll_x = 7;
        app.log_file_select_down();
        assert_eq!(app.log_view.file_selected, 1);
        assert_eq!(app.log_view.file_scroll_x, 0);
    }

    #[test]
    fn diff_scroll_routes_to_file_view_when_in_file_mode() {
        let mut app = app_with_files(vec![]);
        app.diff.scroll_x = 12;
        app.diff.file_view.scroll_x = 4;
        app.diff.view = DiffPaneView::File;

        app.diff.scroll_right();
        assert_eq!(app.diff.scroll_x, 12, "diff scroll_x must not change");
        assert_eq!(app.diff.file_view.scroll_x, 8);

        app.diff.scroll_left();
        assert_eq!(app.diff.file_view.scroll_x, 4);

        app.diff.view = DiffPaneView::Diff;
        app.diff.scroll_right();
        assert_eq!(app.diff.scroll_x, 16);
        assert_eq!(
            app.diff.file_view.scroll_x, 4,
            "file_view scroll_x must not change in diff mode"
        );
    }

    #[test]
    fn selected_filtered_status_file_returns_none_outside_filter() {
        let mut app = app_with_files(vec!["alpha.rs", "bravo.rs", "charlie.rs"]);
        app.status_view.search_query = "alpha".into();
        app.status_view.search_query_lower = "alpha".into();
        app.status_view.recompute_filter();
        // Filter only matches index 0; selecting index 2 must return None.
        app.status_view.selected = 2;
        assert!(app.selected_filtered_status_file().is_none());

        app.status_view.selected = 0;
        assert_eq!(
            app.selected_filtered_status_file().map(|f| f.path.as_str()),
            Some("alpha.rs")
        );
    }

    #[test]
    fn strip_escape_sequences_preserves_user_keystroke_after_bare_esc() {
        // ESC followed by an ordinary character was previously consumed; the
        // letter must now survive so user input echoed via PTY isn't lost.
        let out = super::strip_escape_sequences(b"\x1bA");
        assert_eq!(out, "A");
    }

    #[test]
    fn strip_escape_sequences_drops_csi_and_ss3() {
        // CSI (cursor key), SS3 (alternate keypad), and charset designation
        // must all be stripped fully without leaving final bytes behind.
        let out = super::strip_escape_sequences(b"hi\x1b[31mRED\x1b[0m\x1bOA\x1b(Bend");
        assert_eq!(out, "hiREDend");
    }

    #[test]
    fn strip_escape_sequences_keeps_text_after_malformed_ss3() {
        // ESC O followed by a control byte is not a valid SS3 sequence. The
        // old implementation unconditionally consumed two chars after ESC,
        // swallowing the newline (and any subsequent text relying on it).
        let out = super::strip_escape_sequences(b"\x1bO\nhello");
        assert_eq!(out, "\nhello");
    }

    #[test]
    fn strip_escape_sequences_drops_osc_until_terminator() {
        let bel = super::strip_escape_sequences(b"\x1b]0;title\x07ok");
        assert_eq!(bel, "ok");
        let st = super::strip_escape_sequences(b"\x1b]0;title\x1b\\ok");
        assert_eq!(st, "ok");
    }

    #[test]
    fn keep_scroll_clamps_when_new_diff_is_shorter() {
        let mut app = app_with_files(vec!["a.rs"]);
        // Seed a long diff and put scroll near the bottom.
        app.diff.hunks = vec![
            context_hunk(&["l1", "l2", "l3", "l4", "l5"]),
            context_hunk(&["l6", "l7", "l8"]),
        ];
        app.diff.scroll = app.diff.max_scroll();
        let prev_scroll = app.diff.scroll;
        assert!(prev_scroll > 1);

        // Apply a much shorter diff with KeepScroll; scroll must clamp.
        let shorter = vec![context_hunk(&["only"])];
        app.apply_diff_result(Ok(shorter), DiffApply::KeepScroll(prev_scroll));
        assert!(
            app.diff.scroll <= app.diff.max_scroll(),
            "scroll {} exceeded max {}",
            app.diff.scroll,
            app.diff.max_scroll()
        );
    }

    #[test]
    fn toggle_diff_file_view_ignores_selection_outside_filter() {
        let mut app = app_with_files(vec!["alpha.rs", "bravo.rs"]);
        app.status_view.search_query = "alpha".into();
        app.status_view.search_query_lower = "alpha".into();
        app.status_view.recompute_filter();
        // selected points outside the filter — toggle must refuse to open
        // a file view rather than loading the hidden entry.
        app.status_view.selected = 1;
        app.toggle_diff_file_view();
        assert_eq!(app.diff.view, DiffPaneView::Diff);
        assert!(app.diff.file_view.key.is_none());
    }

    /// Helper: build a populated FileViewState so tests can assert that
    /// downstream operations either preserve or invalidate it without
    /// going through the disk-reading `load_file_view` path.
    fn seeded_file_view(path: &str) -> FileViewState {
        FileViewState {
            key: Some(FileViewKey::Status(path.to_string())),
            content: "one\ntwo\nthree\n".to_string(),
            scroll: 1,
            scroll_x: 4,
            total_lines: 3,
            ..Default::default()
        }
    }

    #[test]
    fn keep_scroll_preserves_open_file_view() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["l1", "l2"])];
        app.diff.scroll = 1;
        app.diff.file_view = seeded_file_view("a.rs");
        app.diff.view = DiffPaneView::File;

        // Same file refresh through KeepScroll must leave the file view
        // alone — only Reset paths should invalidate it.
        let fresh = vec![context_hunk(&["l1", "l2", "l3"])];
        app.apply_diff_result(Ok(fresh), DiffApply::KeepScroll(app.diff.scroll));

        assert_eq!(app.diff.view, DiffPaneView::File);
        assert_eq!(
            app.diff.file_view.key,
            Some(FileViewKey::Status("a.rs".into()))
        );
        assert_eq!(app.diff.file_view.scroll, 1);
        assert_eq!(app.diff.file_view.scroll_x, 4);
    }

    #[test]
    fn clear_diff_state_invalidates_open_file_view() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.diff.hunks = vec![context_hunk(&["l1"])];
        app.diff.file_view = seeded_file_view("a.rs");
        app.diff.view = DiffPaneView::File;

        // toggle_mode and other reset paths route through clear_diff_state
        // — that single call must wipe the file view to its default.
        app.clear_diff_state();

        assert_eq!(app.diff.view, DiffPaneView::Diff);
        assert!(app.diff.file_view.key.is_none());
        assert!(app.diff.file_view.content.is_empty());
        assert_eq!(app.diff.file_view.scroll, 0);
        assert_eq!(app.diff.file_view.scroll_x, 0);
    }

    #[test]
    fn snapshot_refresh_with_no_filter_matches_clears_file_view() {
        let (snapshot, tx) = dummy_snapshot_channel();
        let mut app = App {
            snapshot,
            ..app_with_files(vec!["bar.rs"])
        };
        app.status_view.search_query = "bar".into();
        app.status_view.search_query_lower = "bar".into();
        app.status_view.recompute_filter();
        app.diff.hunks = vec![context_hunk(&["stale"])];
        app.diff.file_view = seeded_file_view("bar.rs");
        app.diff.view = DiffPaneView::File;

        tx.send(SnapshotMsg::Ok(
            RepoSnapshot {
                files: vec![ChangedFile::new(
                    "aaa.rs".to_string(),
                    ChangeStatus::Modified,
                )],
                tracking: None,
            },
            HashMap::new(),
        ))
        .unwrap();
        app.poll_snapshot();

        // No filter matches the new snapshot, so the diff and file view
        // both need to drop their stale handles on the gone path.
        assert!(app.filtered_indices().is_empty());
        assert!(app.diff.hunks.is_empty());
        assert_eq!(app.diff.view, DiffPaneView::Diff);
        assert!(app.diff.file_view.key.is_none());
    }

    fn snapshot_with(paths: &[&str]) -> RepoSnapshot {
        RepoSnapshot {
            files: paths
                .iter()
                .map(|p| ChangedFile::new((*p).to_string(), ChangeStatus::Modified))
                .collect(),
            tracking: None,
        }
    }

    #[test]
    fn ingest_snapshot_populates_hot_table_from_mtimes() {
        let mut app = app_with_files(vec![]);
        let snap = snapshot_with(&["a.rs", "b.rs"]);
        let now = SystemTime::now();
        let mtimes = HashMap::from([
            ("a.rs".to_string(), now),
            ("b.rs".to_string(), now - Duration::from_secs(5)),
        ]);

        app.ingest_snapshot(snap, mtimes);

        assert_eq!(app.status_view.hot_table.len(), 2);
        assert!(app.status_view.hot_table.contains_key("a.rs"));
        assert!(app.status_view.hot_table.contains_key("b.rs"));
    }

    #[test]
    fn merge_hot_table_drops_paths_missing_from_new_snapshot() {
        let mut app = app_with_files(vec![]);
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs"]),
            HashMap::from([("a.rs".to_string(), now)]),
        );
        assert!(app.status_view.hot_table.contains_key("a.rs"));

        app.ingest_snapshot(snapshot_with(&["b.rs"]), HashMap::new());
        assert!(!app.status_view.hot_table.contains_key("a.rs"));
        assert!(!app.status_view.hot_table.contains_key("b.rs"));
    }

    #[test]
    fn merge_hot_table_replaces_only_when_newer() {
        let mut app = app_with_files(vec![]);
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let newer = SystemTime::UNIX_EPOCH + Duration::from_secs(200);

        app.ingest_snapshot(
            snapshot_with(&["a.rs"]),
            HashMap::from([("a.rs".to_string(), newer)]),
        );
        app.ingest_snapshot(
            snapshot_with(&["a.rs"]),
            HashMap::from([("a.rs".to_string(), old)]),
        );

        // The earlier mtime must not overwrite the newer observation; a
        // rename-from-stash scenario can resurrect older mtimes for the
        // same path and would otherwise demote a fresh edit to cool.
        assert_eq!(app.status_view.hot_table.get("a.rs"), Some(&newer));
    }

    #[test]
    fn auto_follow_selects_freshest_hot_file_when_idle() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.selected = 0;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([
                ("a.rs".to_string(), now - Duration::from_secs(5)),
                ("b.rs".to_string(), now),
            ]),
        );

        // b.rs is fresher and the user is idle (last_manual_nav_at = None),
        // so selection must move from a.rs to b.rs.
        assert_eq!(app.status_view.selected, 1);
        assert_eq!(app.auto_followed_path.as_deref(), Some("b.rs"));
    }

    #[test]
    fn auto_follow_skipped_when_user_recently_navigated() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.selected = 0;
        app.last_manual_nav_at = Some(Instant::now());
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        assert_eq!(app.status_view.selected, 0);
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn auto_follow_skipped_when_focus_not_filelist() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.focus = Focus::DiffViewer;
        app.status_view.selected = 0;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        assert_eq!(app.status_view.selected, 0);
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn auto_follow_skipped_when_disabled_in_config() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.cfg_agent_indicator.auto_follow = false;
        app.status_view.selected = 0;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        assert_eq!(app.status_view.selected, 0);
    }

    #[test]
    fn auto_follow_skipped_when_freshest_is_already_selected() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.status_view.selected = 1;
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["a.rs", "b.rs"]),
            HashMap::from([("b.rs".to_string(), now)]),
        );

        // Selection already points to b.rs — no need to steer or arm the
        // "already followed here" guard.
        assert_eq!(app.status_view.selected, 1);
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn select_down_marks_user_active_when_focus_is_filelist() {
        let mut app = app_with_files(vec!["a.rs", "b.rs"]);
        app.focus = Focus::FileList;
        app.auto_followed_path = Some("a.rs".to_string());

        app.select_down();

        assert!(app.last_manual_nav_at.is_some());
        assert!(app.auto_followed_path.is_none());
    }

    #[test]
    fn select_down_does_not_mark_when_focus_is_diff() {
        let mut app = app_with_files(vec!["a.rs"]);
        app.focus = Focus::DiffViewer;

        app.select_down();

        assert!(app.last_manual_nav_at.is_none());
    }

    #[test]
    fn auto_follow_respects_search_filter() {
        let mut app = app_with_files(vec!["alpha.rs", "beta.rs"]);
        app.status_view.search_query = "alpha".into();
        app.status_view.search_query_lower = "alpha".into();
        app.status_view.recompute_filter();
        app.status_view.selected = 0; // alpha.rs (the only filtered entry)
        let now = SystemTime::now();

        app.ingest_snapshot(
            snapshot_with(&["alpha.rs", "beta.rs"]),
            HashMap::from([
                ("alpha.rs".to_string(), now - Duration::from_secs(5)),
                ("beta.rs".to_string(), now),
            ]),
        );

        // beta.rs is fresher but filtered out, so auto-follow must not
        // jump to a row the user cannot even see.
        assert_eq!(app.status_view.selected, 0);
    }
}
