use crate::git::diff::StatusKind;
use crate::ui::diff_pane::{
    DIFF_THEME, HIGHLIGHT_LINE_CAP, HighlightSegment, highlight_line_segments,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileViewKey {
    Status(String),
    Commit {
        oid: git2::Oid,
        path: String,
        status: StatusKind,
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
    /// Cached syntect highlight output for the leading
    /// [`HIGHLIGHT_LINE_CAP`] lines, built once per (content, syntax) so
    /// per-frame rendering only slices the visible window.
    pub line_highlights: Vec<Vec<HighlightSegment>>,
    /// Syntax name used to build `line_highlights`. `None` = unbuilt or
    /// invalidated.
    pub cached_syntax_name: Option<String>,
    /// Cached `content.lines().count()` populated on load; `max_scroll` is
    /// called from every j/k/PgUp/PgDn keystroke, so walking the file per
    /// keystroke is not viable.
    pub(crate) total_lines: usize,
    /// Byte length of `content` at cache build time. With `total_lines` it
    /// detects in-place content edits that keep the line count constant.
    pub(crate) cached_content_len: usize,
    /// Byte offset of each logical line start, built together with `content`
    /// so a line past the highlight cap can be sliced without rescanning.
    pub(crate) line_starts: Vec<usize>,
    /// Lowercased copy of each `content` line. Built on demand by
    /// `ensure_lower_cache` so per-keystroke file search avoids re-lowercasing.
    pub(crate) lines_lower: Vec<String>,
}

impl FileViewState {
    pub fn line_count(&self) -> usize {
        self.total_lines
    }

    /// Replace the rendered content, keeping `total_lines` and the highlight
    /// cache in lockstep with `content` so partial assignments at call sites
    /// can't leave them disagreeing (which would make `max_scroll` lie about
    /// the legal scroll range).
    pub fn set_content(&mut self, content: String) {
        self.line_starts = line_starts(&content);
        self.total_lines = self.line_starts.len();
        self.content = content;
        self.line_highlights.clear();
        self.cached_syntax_name = None;
        self.cached_content_len = 0;
        self.lines_lower.clear();
        self.scroll = self.scroll.min(self.max_scroll());
        self.error = None;
    }

    pub fn max_scroll(&self) -> usize {
        self.line_count().saturating_sub(1)
    }

    pub fn scroll_up(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_sub(n);
    }

    pub fn scroll_down(&mut self, n: usize) {
        self.scroll = self.scroll.saturating_add(n).min(self.max_scroll());
    }

    /// Text of the logical line at `idx`, without its line terminator, or
    /// `None` when `idx` is past the last line.
    pub(crate) fn line_text(&self, idx: usize) -> Option<&str> {
        let start = *self.line_starts.get(idx)?;
        let end = self
            .line_starts
            .get(idx + 1)
            .copied()
            .unwrap_or(self.content.len());
        let line = &self.content[start..end];
        let line = line.strip_suffix('\n').unwrap_or(line);
        Some(line.strip_suffix('\r').unwrap_or(line))
    }

    /// Ensure `lines_lower` is built for the current `content`, so per-
    /// keystroke search pays the lowercase cost once per file load.
    pub(crate) fn ensure_lower_cache(&mut self) {
        if self.lines_lower.len() == self.total_lines && !self.content.is_empty() {
            return;
        }
        self.lines_lower = self.content.lines().map(|l| l.to_lowercase()).collect();
    }

    /// Ensure the leading `line_highlights` match the current `content` and
    /// supplied syntax, stopping at [`HIGHLIGHT_LINE_CAP`]. Rebuilds when the
    /// cached prefix length diverges or the syntax name changed since the
    /// last build.
    pub fn ensure_highlight_cache(
        &mut self,
        ss: &syntect::parsing::SyntaxSet,
        ts: &syntect::highlighting::ThemeSet,
        syntax: &syntect::parsing::SyntaxReference,
    ) {
        let limit = self.line_count().min(HIGHLIGHT_LINE_CAP);
        let content_len = self.content.len();
        if self.line_highlights.len() == limit
            && self.cached_content_len == content_len
            && self.cached_syntax_name.as_deref() == Some(syntax.name.as_str())
        {
            return;
        }

        use syntect::easy::HighlightLines;
        let theme = &ts.themes[DIFF_THEME];
        let mut hl = HighlightLines::new(syntax, theme);

        let mut out: Vec<Vec<HighlightSegment>> = Vec::with_capacity(limit);
        for raw in self.content.lines().take(limit) {
            out.push(highlight_line_segments(&mut hl, ss, raw));
        }
        self.line_highlights = out;
        self.cached_syntax_name = Some(syntax.name.clone());
        self.cached_content_len = content_len;
    }
}

/// Byte offsets where `str::lines` starts each line. A trailing newline does
/// not start an empty final line, matching `str::lines`.
fn line_starts(content: &str) -> Vec<usize> {
    if content.is_empty() {
        return Vec::new();
    }
    let bytes = content.as_bytes();
    let mut starts = vec![0];
    for (i, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' && i + 1 < bytes.len() {
            starts.push(i + 1);
        }
    }
    starts
}

#[cfg(test)]
#[path = "file_view_tests.rs"]
mod tests;
