use crate::git::diff::LineKind;
use crate::ui::diff_pane::{DiffPane, DiffPaneView, SplitRow, flush_split_blocks, highlight_line_segments, nearest_match_index, resolve_hunk_syntax, DIFF_THEME, HighlightSegment};

impl DiffPane {
    /// Total flat row count across all hunks (1 header + N body lines each).
    pub fn line_count(&self) -> usize {
        self.hunks.iter().map(|h| 1 + h.lines.len()).sum()
    }

    /// Largest legal `scroll` value: one less than the total row count, or 0
    /// when there are no rows.
    pub fn max_scroll(&self) -> usize {
        self.line_count().saturating_sub(1)
    }

    /// Move the active horizontal scroll target left by one tab stop.
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
            // Split reuses the unified diff's horizontal cursor so both halves
            // scroll together.
            DiffPaneView::Diff | DiffPaneView::Split => &mut self.scroll_x,
        }
    }

    /// Build the side-by-side row layout from the current hunks. Within each
    /// hunk, consecutive removed/added lines are paired index-by-index (the
    /// shorter run padded with blank cells), and context lines are mirrored.
    /// Cheap to recompute: it only walks line kinds and stores coordinates.
    pub fn split_rows(&self) -> Vec<SplitRow> {
        let mut rows = Vec::new();
        for (hi, hunk) in self.hunks.iter().enumerate() {
            rows.push(SplitRow::Header(hi));
            let mut removed: Vec<usize> = Vec::new();
            let mut added: Vec<usize> = Vec::new();
            for (li, line) in hunk.lines.iter().enumerate() {
                match line.kind {
                    LineKind::Removed => removed.push(li),
                    LineKind::Added => added.push(li),
                    LineKind::Context => {
                        flush_split_blocks(&mut rows, hi, &mut removed, &mut added);
                        rows.push(SplitRow::Body {
                            left: Some((hi, li)),
                            right: Some((hi, li)),
                        });
                    }
                }
            }
            flush_split_blocks(&mut rows, hi, &mut removed, &mut added);
        }
        rows
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
            if self.view == DiffPaneView::File {
                self.file_view.scroll = idx.min(self.file_view.max_scroll());
            } else {
                self.scroll = idx;
            }
        }
    }

    pub fn prev_match(&mut self) {
        if let Some(idx) = self.search.prev() {
            if self.view == DiffPaneView::File {
                self.file_view.scroll = idx.min(self.file_view.max_scroll());
            } else {
                self.scroll = idx;
            }
        }
    }

    /// Rebuild `search.matches` against the current query, using
    /// `hunks_lines_lower` so per-keystroke search is just a substring scan
    /// over precomputed strings. `scroll_to_match=true` jumps the viewport to
    /// the current cursor's match (after a keystroke); `false` keeps the
    /// viewport pinned and re-anchors `cursor` to the nearest match (a
    /// content-only refresh, e.g. a background snapshot tick while a query is
    /// active, so the next `n`/`p` does not jump unexpectedly).
    pub fn recompute_matches(&mut self, scroll_to_match: bool) {
        self.search.matches.clear();
        if self.search.query.is_empty() {
            self.search.cursor = 0;
            return;
        }
        let q_owned;
        let q: &str;
        if self.view == DiffPaneView::File {
            self.file_view.ensure_lower_cache();
            q_owned = self.search.query.lower().to_owned();
            q = &q_owned;
            for (idx, line_lower) in self.file_view.lines_lower.iter().enumerate() {
                if line_lower.contains(q) {
                    self.search.matches.push(idx);
                }
            }
        } else {
            self.ensure_lower_cache();
            q_owned = self.search.query.lower().to_owned();
            q = &q_owned;
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
        }
        debug_assert!(
            self.search.matches.windows(2).all(|w| w[0] < w[1]),
            "diff_search_matches must be sorted for binary_search to be correct"
        );
        if self.search.matches.is_empty() {
            self.search.cursor = 0;
            return;
        }
        if scroll_to_match {
            self.search.cursor = self.search.cursor.min(self.search.matches.len() - 1);
            self.scroll_to_match();
        } else {
            let anchor = if self.view == DiffPaneView::File {
                self.file_view.scroll
            } else {
                self.scroll
            };
            self.search.cursor = nearest_match_index(&self.search.matches, anchor);
        }
    }

    #[cfg(test)]
    pub(crate) fn search_cursor(&self) -> usize {
        self.search.cursor
    }

    fn scroll_to_match(&mut self) {
        let Some(&idx) = self.search.matches.get(self.search.cursor) else {
            return;
        };
        if self.view == DiffPaneView::File {
            self.file_view.scroll = idx.min(self.file_view.max_scroll());
        } else {
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
        self.line_highlights.clear();
        self.cached_hunk_syntax.clear();
    }

    /// Rebuild the lowercased line cache iff its shape diverges from `hunks`.
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

    /// Ensure `line_highlights` matches the current `hunks`, resolving the
    /// syntax separately for each hunk from its `file_path`. A commit diff
    /// can touch files of different types — using a single syntax for the
    /// whole diff would render everything as the first file's language (or
    /// plain text). Rebuilds when the cache shape, content size, or any
    /// per-hunk syntax diverges.
    pub fn ensure_highlight_cache(
        &mut self,
        ss: &syntect::parsing::SyntaxSet,
        ts: &syntect::highlighting::ThemeSet,
    ) {
        let per_hunk_syntax: Vec<&syntect::parsing::SyntaxReference> = self
            .hunks
            .iter()
            .map(|h| resolve_hunk_syntax(ss, h.file_path.as_deref()))
            .collect();
        let resolved_names: Vec<String> = per_hunk_syntax.iter().map(|s| s.name.clone()).collect();

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
            && self.cached_hunk_syntax == resolved_names
        {
            return;
        }

        use syntect::easy::HighlightLines;
        let theme = &ts.themes[DIFF_THEME];
        // Reset the highlighter state pair whenever the hunk's syntax
        // changes — running a JS hunk through a Rust HighlightLines would
        // mis-paint stateful multi-line constructs.
        let mut hl_pair: Option<(HighlightLines<'_>, HighlightLines<'_>)> = None;
        let mut current_syntax_name = String::new();

        let mut out: Vec<Vec<Vec<HighlightSegment>>> = Vec::with_capacity(self.hunks.len());
        for (hunk, syntax) in self.hunks.iter().zip(per_hunk_syntax.iter()) {
            if hl_pair.is_none() || current_syntax_name != syntax.name {
                hl_pair = Some((
                    HighlightLines::new(syntax, theme),
                    HighlightLines::new(syntax, theme),
                ));
                current_syntax_name = syntax.name.clone();
            }
            // Safe: just assigned above when None.
            let (hl_new, hl_old) = hl_pair.as_mut().unwrap();

            let mut per_hunk: Vec<Vec<HighlightSegment>> = Vec::with_capacity(hunk.lines.len());
            for line in &hunk.lines {
                let hl = match line.kind {
                    LineKind::Removed => &mut *hl_old,
                    _ => &mut *hl_new,
                };
                per_hunk.push(highlight_line_segments(hl, ss, &line.content));
            }
            out.push(per_hunk);
        }
        self.line_highlights = out;
        self.cached_hunk_syntax = resolved_names;
        self.cached_content_bytes = content_bytes;
    }
}
