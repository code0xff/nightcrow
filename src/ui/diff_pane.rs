use crate::git::diff::{DiffHunk, LineKind};
use crate::ui::SearchQuery;
use crate::ui::file_view::FileViewState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffPaneView {
    #[default]
    Diff,
    File,
    /// Side-by-side diff: removed lines on the left, added lines on the right,
    /// context lines mirrored on both sides. Falls back to the unified `Diff`
    /// renderer when the pane is too narrow to split usefully.
    Split,
}

/// One row of the side-by-side layout. `Header` carries the hunk index whose
/// `@@ ... @@` header spans the full width; `Body` carries the (hunk, line)
/// coordinates shown on each side, with `None` marking a blank padding cell
/// where one side has no counterpart line. Coordinates index into
/// `DiffPane::hunks` (and the matching `line_highlights`) so the renderer can
/// reuse the prebuilt highlight cache without re-running syntect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitRow {
    Header(usize),
    Body {
        left: Option<(usize, usize)>,
        right: Option<(usize, usize)>,
    },
}

#[derive(Default)]
pub struct DiffSearch {
    pub active: bool,
    pub query: SearchQuery,
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
        self.matches.clear();
        self.cursor = 0;
    }

    fn push_char(&mut self, ch: char) {
        self.query.push(ch);
    }

    fn pop_char(&mut self) {
        self.query.pop();
    }

    fn next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        // Defensive clamp: `recompute_matches(false)` re-anchors `cursor` to
        // the nearest match, but a stale cursor can otherwise survive into
        // here through code paths that mutate `matches` without re-anchoring.
        if self.cursor >= self.matches.len() {
            self.cursor = 0;
        } else {
            self.cursor = (self.cursor + 1) % self.matches.len();
        }
        self.current_match()
    }

    fn prev(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        if self.cursor == 0 || self.cursor >= self.matches.len() {
            self.cursor = self.matches.len() - 1;
        } else {
            self.cursor -= 1;
        }
        self.current_match()
    }
}

/// Return the index of the match in `matches` whose flat row is closest to
/// `scroll`. Ties prefer the smaller flat row (i.e. the one already on or
/// above the cursor) so a content refresh during reading never jumps the
/// "current match" past where the user is looking. `matches` must be sorted
/// ascending and non-empty.
fn nearest_match_index(matches: &[usize], scroll: usize) -> usize {
    debug_assert!(!matches.is_empty());
    match matches.binary_search(&scroll) {
        Ok(i) => i,
        Err(i) => {
            if i == 0 {
                0
            } else if i == matches.len() {
                matches.len() - 1
            } else {
                let prev = matches[i - 1];
                let next = matches[i];
                if scroll - prev <= next - scroll {
                    i - 1
                } else {
                    i
                }
            }
        }
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

/// Run a single line through the supplied syntect highlighter and convert
/// the result into `HighlightSegment`s. Falls back to a single grey segment
/// on highlighter error. Shared by `DiffPane` and `FileViewState` so both
/// caches build segments identically.
pub(crate) fn highlight_line_segments(
    hl: &mut syntect::easy::HighlightLines,
    ss: &syntect::parsing::SyntaxSet,
    raw: &str,
) -> Vec<HighlightSegment> {
    // syntect expects trailing newlines to terminate lines; strip them back
    // off the resulting segments so cached text matches the source line.
    let with_nl = format!("{raw}\n");
    match hl.highlight_line(&with_nl, ss) {
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
            text: raw.to_string(),
        }],
    }
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
    /// Syntax name (`SyntaxReference::name`) resolved per hunk at the time
    /// `line_highlights` was built. Stored as a per-hunk vector because a
    /// single commit diff can touch files of different types and each hunk
    /// needs its own highlighter state. Empty means the cache is unbuilt
    /// or invalidated.
    pub cached_hunk_syntax: Vec<String>,
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
            // Split reuses the unified diff's horizontal cursor so both halves
            // scroll together.
            DiffPaneView::Diff | DiffPaneView::Split => &mut self.scroll_x,
        }
    }

    /// Build the side-by-side row layout from the current hunks. Within each
    /// hunk, consecutive removed/added lines are paired index-by-index (the
    /// shorter run padded with blank cells), and context lines are mirrored on
    /// both sides. Cheap to recompute: it only walks line kinds and stores
    /// coordinates, never copying content.
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
    /// over precomputed strings.
    ///
    /// `scroll_to_match` selects the post-rebuild behaviour:
    /// - `true`: jump the viewport to the current cursor's match (used after
    ///   a keystroke where the user explicitly drove the search).
    /// - `false`: keep the viewport pinned and re-anchor `cursor` to the
    ///   match nearest to the current scroll. Without this, a content-only
    ///   refresh (e.g. background snapshot tick while a query is active)
    ///   would leave the "current match" indicator at a stale row far from
    ///   where the user is reading, so the next `n`/`p` would jump
    ///   unexpectedly.
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
        // Highlight cache shape is keyed by hunks; invalidate so the renderer
        // rebuilds it on next frame against the active syntax.
        self.line_highlights.clear();
        self.cached_hunk_syntax.clear();
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

    /// Ensure `line_highlights` matches the current `hunks`, resolving the
    /// syntax separately for each hunk from its `file_path`. A commit diff
    /// can touch files of different types — using a single syntax for the
    /// whole diff would render everything as the first file's language (or
    /// plain text, when there is no single "current" file). Rebuilds when
    /// the cache shape, content size, or any per-hunk syntax diverges from
    /// the cached state.
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
            // Safe: just assigned in the line above when None.
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

/// Flush the pending removed/added runs into paired `SplitRow::Body` rows,
/// padding the shorter run with `None` cells, then clear both queues. Called
/// whenever a context line or hunk boundary breaks a change block.
fn flush_split_blocks(
    rows: &mut Vec<SplitRow>,
    hi: usize,
    removed: &mut Vec<usize>,
    added: &mut Vec<usize>,
) {
    let pairs = removed.len().max(added.len());
    for i in 0..pairs {
        rows.push(SplitRow::Body {
            left: removed.get(i).map(|&li| (hi, li)),
            right: added.get(i).map(|&li| (hi, li)),
        });
    }
    removed.clear();
    added.clear();
}

/// Pick the syntect syntax for a hunk based on its `file_path`'s extension.
/// Falls back to plain text when the path is absent (test fixtures) or the
/// extension is unknown.
fn resolve_hunk_syntax<'a>(
    ss: &'a syntect::parsing::SyntaxSet,
    file_path: Option<&str>,
) -> &'a syntect::parsing::SyntaxReference {
    file_path
        .map(crate::ui::path_extension)
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::diff::{DiffLine, LineKind};

    #[test]
    fn nearest_match_index_picks_closest_and_prefers_lower_on_tie() {
        let m = [10, 30, 50];
        assert_eq!(nearest_match_index(&m, 5), 0);
        assert_eq!(nearest_match_index(&m, 10), 0);
        assert_eq!(nearest_match_index(&m, 19), 0);
        // tie: equidistant from 10 and 30 → prefer the lower row.
        assert_eq!(nearest_match_index(&m, 20), 0);
        assert_eq!(nearest_match_index(&m, 21), 1);
        assert_eq!(nearest_match_index(&m, 50), 2);
        assert_eq!(nearest_match_index(&m, 999), 2);
    }

    fn match_hunk(lines: &[&str]) -> DiffHunk {
        DiffHunk {
            header: "@@".to_string(),
            lines: lines
                .iter()
                .map(|s| DiffLine {
                    kind: LineKind::Context,
                    content: (*s).to_string(),
                })
                .collect(),
            file_path: None,
        }
    }

    #[test]
    fn recompute_matches_keep_scroll_repins_cursor_near_viewport() {
        // 1 hunk header + 10 body lines. "foo" matches at body indices 0, 4, 8
        // → flat rows 1, 5, 9.
        let mut pane = DiffPane {
            hunks: vec![match_hunk(&[
                "foo a", "b", "c", "d", "foo e", "f", "g", "h", "foo i", "j",
            ])],
            ..Default::default()
        };
        pane.search.query.set("foo");
        pane.scroll = 6; // user is reading near the middle match (row 5)
        pane.search.cursor = 0; // stale cursor from before content changed

        pane.recompute_matches(false);

        assert_eq!(pane.search.matches, vec![1, 5, 9]);
        // Closest match to scroll=6 is row 5 (cursor index 1), not the
        // stale index 0 or a clamp to len-1.
        assert_eq!(pane.search_cursor(), 1);
        // Viewport stayed pinned where the user left it.
        assert_eq!(pane.scroll, 6);
    }

    #[test]
    fn recompute_matches_scroll_to_match_clamps_and_jumps() {
        let mut pane = DiffPane {
            hunks: vec![match_hunk(&["foo a", "b", "foo c"])],
            ..Default::default()
        };
        pane.search.query.set("foo");
        pane.scroll = 100; // arbitrary; scroll_to_match should overwrite
        pane.search.cursor = 99; // stale, should clamp to last match index.

        pane.recompute_matches(true);

        assert_eq!(pane.search.matches, vec![1, 3]);
        assert_eq!(pane.search_cursor(), 1);
        assert_eq!(pane.scroll, 3);
    }

    fn kinded_hunk(lines: &[(LineKind, &str)]) -> DiffHunk {
        DiffHunk {
            header: "@@".to_string(),
            lines: lines
                .iter()
                .map(|(kind, s)| DiffLine {
                    kind: *kind,
                    content: (*s).to_string(),
                })
                .collect(),
            file_path: None,
        }
    }

    #[test]
    fn split_rows_pairs_changes_and_mirrors_context() {
        use LineKind::{Added, Context, Removed};
        // A typical edit block: one context line, a 2-removed/1-added change,
        // then a trailing context line.
        let pane = DiffPane {
            hunks: vec![kinded_hunk(&[
                (Context, "ctx0"),
                (Removed, "old a"),
                (Removed, "old b"),
                (Added, "new a"),
                (Context, "ctx1"),
            ])],
            ..Default::default()
        };

        let rows = pane.split_rows();
        assert_eq!(
            rows,
            vec![
                SplitRow::Header(0),
                // context mirrored on both sides
                SplitRow::Body {
                    left: Some((0, 0)),
                    right: Some((0, 0)),
                },
                // removed[0] pairs with added[0]
                SplitRow::Body {
                    left: Some((0, 1)),
                    right: Some((0, 3)),
                },
                // removed[1] has no added counterpart → right padded blank
                SplitRow::Body {
                    left: Some((0, 2)),
                    right: None,
                },
                SplitRow::Body {
                    left: Some((0, 4)),
                    right: Some((0, 4)),
                },
            ]
        );
        // 1 header + 4 body rows.
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn split_rows_pads_added_only_block() {
        use LineKind::Added;
        // Pure insertion: every change row has a blank left side.
        let pane = DiffPane {
            hunks: vec![kinded_hunk(&[(Added, "x"), (Added, "y")])],
            ..Default::default()
        };
        let rows = pane.split_rows();
        assert_eq!(
            rows,
            vec![
                SplitRow::Header(0),
                SplitRow::Body {
                    left: None,
                    right: Some((0, 0)),
                },
                SplitRow::Body {
                    left: None,
                    right: Some((0, 1)),
                },
            ]
        );
    }

    fn make_file_view_pane(content: &str) -> DiffPane {
        let mut pane = DiffPane {
            view: DiffPaneView::File,
            ..Default::default()
        };
        pane.file_view.set_content(content.to_string());
        pane
    }

    #[test]
    fn file_view_search_matches_correct_line_indices() {
        let mut pane = make_file_view_pane("hello world\nfoo bar\nhello again\n");
        for ch in "hello".chars() {
            pane.search_push(ch);
        }
        // lines 0 and 2 contain "hello"
        assert_eq!(pane.search.matches, vec![0, 2]);
    }

    #[test]
    fn file_view_search_no_matches() {
        let mut pane = make_file_view_pane("foo\nbar\nbaz\n");
        for ch in "xyz".chars() {
            pane.search_push(ch);
        }
        assert!(pane.search.matches.is_empty());
    }

    #[test]
    fn file_view_search_case_insensitive() {
        let mut pane = make_file_view_pane("Hello World\nhello\nHELLO\n");
        for ch in "hello".chars() {
            pane.search_push(ch);
        }
        assert_eq!(pane.search.matches, vec![0, 1, 2]);
    }

    #[test]
    fn file_view_next_match_updates_file_scroll() {
        let mut pane = make_file_view_pane("match\nskip\nmatch\n");
        for ch in "match".chars() {
            pane.search_push(ch);
        }
        assert_eq!(pane.file_view.scroll, 0); // jumped to first match
        pane.next_match();
        assert_eq!(pane.file_view.scroll, 2); // jumped to second match
        pane.next_match();
        assert_eq!(pane.file_view.scroll, 0); // wraps back to first
    }

    #[test]
    fn file_view_prev_match_updates_file_scroll() {
        let mut pane = make_file_view_pane("match\nskip\nmatch\n");
        for ch in "match".chars() {
            pane.search_push(ch);
        }
        pane.prev_match();
        assert_eq!(pane.file_view.scroll, 2); // wraps to last match
    }

    #[test]
    fn file_view_search_clear_resets_state() {
        let mut pane = make_file_view_pane("hello\nworld\n");
        for ch in "hello".chars() {
            pane.search_push(ch);
        }
        assert!(!pane.search.matches.is_empty());
        pane.cancel_search();
        assert!(pane.search.matches.is_empty());
        assert!(!pane.search.active);
    }
}
