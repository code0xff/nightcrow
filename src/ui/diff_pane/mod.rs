mod highlight;
mod search;
mod split;
#[cfg(test)]
mod tests;

pub use highlight::{DIFF_THEME, HighlightSegment};
pub use search::DiffSearch;
pub(crate) use highlight::highlight_line_segments;
pub(crate) use search::nearest_match_index;
pub(crate) use split::{flush_split_blocks, resolve_hunk_syntax};

use crate::git::diff::DiffHunk;
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

mod pane_impl;
