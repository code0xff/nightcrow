use crate::git::diff::ChangeStatus;
use crate::ui::diff_pane::{DIFF_THEME, HighlightSegment, highlight_line_segments};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileViewKey {
    Status(String),
    Commit {
        oid: git2::Oid,
        path: String,
        status: ChangeStatus,
    },
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
    pub(crate) total_lines: usize,
    /// Byte length of `content` at the time `line_highlights` was built.
    /// Combined with `total_lines` it lets `ensure_highlight_cache` notice
    /// in-place content edits that happen to keep the line count constant
    /// (line counts alone are too coarse a fingerprint).
    pub(crate) cached_content_len: usize,
}

impl FileViewState {
    pub fn line_count(&self) -> usize {
        self.total_lines
    }

    /// Replace the rendered content. Keeps `total_lines` and the highlight
    /// cache in lockstep with `content` so partial assignments at call sites
    /// can't leave them disagreeing (which would make `max_scroll` lie about
    /// the legal scroll range). Also clamps `scroll` against the new max and
    /// drops any prior error so an in-place reload of the same `FileViewState`
    /// never lands on a row past the new file length or keeps a "load failed"
    /// banner over fresh content.
    pub fn set_content(&mut self, content: String) {
        self.total_lines = if content.is_empty() {
            0
        } else {
            content.lines().count()
        };
        self.content = content;
        // Highlights are content-derived: stale entries would either index
        // past `total_lines` or render the previous file's colors.
        self.line_highlights.clear();
        self.cached_syntax_name = None;
        self.cached_content_len = 0;
        self.scroll = self.scroll.min(self.max_scroll());
        self.error = None;
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
            out.push(highlight_line_segments(&mut hl, ss, raw));
        }
        self.line_highlights = out;
        self.cached_syntax_name = Some(syntax.name.clone());
        self.cached_content_len = content_len;
    }
}
