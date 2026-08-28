use crate::git::diff::LineKind;
use crate::ui::diff_pane::{
    DIFF_THEME, DiffPane, DiffPaneView, HighlightSegment, highlight_line_segments,
    nearest_match_index, resolve_syntax_extension,
};

impl DiffPane {
    pub fn scroll_left(&mut self) {
        let target = self.scroll_x_target_mut();
        *target = target.saturating_sub(4);
    }

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
    /// content-only refresh, e.g. a background snapshot tick, so the next
    /// `n`/`p` does not jump unexpectedly).
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
            for lines_lower in &self.hunks_lines_lower {
                flat_idx += 1; // header line
                for line_lower in lines_lower {
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

    /// Rebuild the lowercased line cache for the current generation. Normal
    /// callers should use `set_hunks`; this remains a recovery hook for tests
    /// and callers that already hold a populated pane.
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
        self.lower_cache_generation = Some(self.generation);
    }

    /// Rebuild the lowercased line cache iff its generation is stale.
    pub fn ensure_lower_cache(&mut self) {
        if self.lower_cache_generation != Some(self.generation) {
            self.rebuild_lower_cache();
        }
    }

    /// Ensure `line_highlights` matches the current generation, resolving the
    /// syntax separately for each hunk: a commit diff can touch files of
    /// different types, and a single syntax would render everything as the
    /// first file's language. The generation check is the frame hot path; the
    /// full syntax/highlight walk happens only after `set_hunks`.
    pub fn ensure_highlight_cache(
        &mut self,
        ss: &syntect::parsing::SyntaxSet,
        ts: &syntect::highlighting::ThemeSet,
    ) {
        if self.highlight_cache_generation == Some(self.generation) {
            return;
        }

        let per_hunk_syntax: Vec<&syntect::parsing::SyntaxReference> = self
            .syntax_shape
            .iter()
            .map(|extension| resolve_syntax_extension(ss, extension.as_deref()))
            .collect();

        use syntect::easy::HighlightLines;
        let theme = &ts.themes[DIFF_THEME];
        // Reset the highlighter state pair whenever the hunk's syntax changes
        // — a JS hunk through a Rust HighlightLines would mis-paint stateful
        // multi-line constructs.
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
        self.highlight_cache_generation = Some(self.generation);
    }
}
