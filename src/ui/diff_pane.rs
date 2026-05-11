use crate::git::diff::{DiffHunk, LineKind};
use crate::ui::file_view::FileViewState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffPaneView {
    #[default]
    Diff,
    File,
}

#[derive(Default)]
pub struct DiffSearch {
    pub active: bool,
    pub query: String,
    pub(crate) query_lower: String,
    pub(crate) matches: Vec<usize>,
    pub(crate) cursor: usize,
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
        // `matches` is built by `recompute_matches` in flat_idx-ascending
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

    pub fn clear(&mut self) {
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
    pub(crate) hunks_lines_lower: Vec<Vec<String>>,
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
    /// `rebuild_lower_cache` invariant.
    pub(crate) cached_content_bytes: usize,
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
