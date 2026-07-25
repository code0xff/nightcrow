mod highlight;
mod search;
mod split;
#[cfg(test)]
mod tests;

pub(crate) use highlight::highlight_line_segments;
pub use highlight::{DIFF_THEME, HighlightSegment};
pub use search::DiffSearch;
pub(crate) use search::nearest_match_index;
pub(crate) use split::{flush_split_blocks, resolve_hunk_syntax};

use crate::git::diff::DiffHunk;
use crate::ui::file_view::FileViewState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffPaneView {
    #[default]
    Diff,
    File,
    /// Side-by-side diff: removed left, added right, context mirrored. Falls
    /// back to the unified `Diff` renderer when the pane is too narrow.
    Split,
}

/// One row of the side-by-side layout. `Header` carries the hunk index whose
/// `@@ ... @@` spans the full width; `Body` carries the (hunk, line)
/// coordinates on each side, with `None` marking a blank padding cell.
/// Coordinates index into `DiffPane::hunks` (and `line_highlights`) so the
/// renderer reuses the prebuilt highlight cache without re-running syntect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitRow {
    Header(usize),
    Body {
        left: Option<(usize, usize)>,
        right: Option<(usize, usize)>,
    },
}

/// All state for the diff viewer pane: hunks, scroll cursors, search state,
/// and the optional file-content overlay.
#[derive(Default)]
pub struct DiffPane {
    pub hunks: Vec<DiffHunk>,
    /// Lowercased copy of each `DiffLine::content` aligned with `hunks`.
    /// Built once per diff load so per-keystroke search does not re-lowercase.
    pub(crate) hunks_lines_lower: Vec<Vec<String>>,
    /// Cached syntect highlight output per body line, same shape as
    /// `hunks_lines_lower`. Built once when hunks (or the active syntax)
    /// change so the renderer skips the full-document state-recovery pass.
    pub line_highlights: Vec<Vec<Vec<HighlightSegment>>>,
    /// Per-hunk syntax name at the time `line_highlights` was built. A commit
    /// diff can touch files of different types, each needing its own
    /// highlighter state. Empty means the cache is unbuilt or invalidated.
    pub cached_hunk_syntax: Vec<String>,
    /// Sum of `line.content.len()` across all hunk lines at cache build time.
    /// Pairs with the shape check so a same-line-count hunk replacement still
    /// invalidates the cache.
    pub(crate) cached_content_bytes: usize,
    pub scroll: usize,
    pub scroll_x: usize,
    pub search: DiffSearch,
    pub view: DiffPaneView,
    pub file_view: FileViewState,
    /// True while the diff pane is rendered full-screen (hint bar excluded).
    /// Mutually exclusive with `TerminalPane::fullscreen`.
    pub fullscreen: bool,
}

mod pane_impl;
