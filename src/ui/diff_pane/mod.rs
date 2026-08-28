mod cache;
mod highlight;
mod search;
mod split;
#[cfg(test)]
mod tests;

pub(crate) use highlight::highlight_line_segments;
pub use highlight::{DIFF_THEME, HighlightSegment};
pub use search::DiffSearch;
pub(crate) use search::nearest_match_index;
pub(crate) use split::{flush_split_blocks, resolve_syntax_extension};

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
/// Coordinates index into `DiffPane::hunks` so the renderer reuses the
/// prebuilt highlight cache without re-running syntect.
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
    /// Loaded hunks. Replaced only through [`DiffPane::set_hunks`] so all
    /// render/search indexes observe one generation boundary.
    hunks: Vec<DiffHunk>,
    /// Lowercased copy of each `DiffLine::content` aligned with `hunks`,
    /// built once per diff load so per-keystroke search does not
    /// re-lowercase.
    pub(crate) hunks_lines_lower: Vec<Vec<String>>,
    /// Cached syntect highlight output per body line. Built once when hunks
    /// (or the active syntax) change so the renderer skips the full-document
    /// state-recovery pass.
    pub line_highlights: Vec<Vec<Vec<HighlightSegment>>>,
    /// Monotonic identity of the currently loaded diff. Derived caches carry
    /// this generation rather than inspecting the hunks during rendering.
    pub(crate) generation: u64,
    /// Flat unified row count, populated together with `hunks`.
    pub(crate) total_lines: usize,
    /// Absolute unified row offset for each hunk, populated with the flat
    /// count so a deep viewport can jump into the owning hunk directly.
    pub(crate) hunk_starts: Vec<usize>,
    /// Largest old/new line number in the loaded diff.
    pub(crate) max_line_number: u32,
    /// File-extension keys used to resolve one syntax per hunk. The keys are
    /// captured at mutation time so a cache hit never walks the hunk list.
    pub(crate) syntax_shape: Vec<Option<String>>,
    /// Cached side-by-side row coordinates. This is rebuilt at mutation time
    /// and borrowed by every split frame.
    pub(crate) split_rows: Vec<SplitRow>,
    /// Generation for the lowercase search cache.
    pub(crate) lower_cache_generation: Option<u64>,
    /// Generation for the syntax-highlight cache.
    pub(crate) highlight_cache_generation: Option<u64>,
    pub scroll: usize,
    pub scroll_x: usize,
    /// Soft-wrap long lines instead of letting them run off the right edge.
    /// Mutually exclusive with horizontal scrolling by construction, not by
    /// choice: ratatui's `Paragraph` ignores its `scroll.x` once wrapping is
    /// on. The split view ignores this entirely — halves that wrap to
    /// different heights would stop lining up.
    pub wrap: bool,
    pub search: DiffSearch,
    pub view: DiffPaneView,
    pub file_view: FileViewState,
    /// True while the diff pane is rendered full-screen (hint bar excluded).
    /// Mutually exclusive with `TerminalPane::fullscreen`.
    pub fullscreen: bool,
}

mod pane_impl;
