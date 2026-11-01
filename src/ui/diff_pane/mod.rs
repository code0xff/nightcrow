mod cache;
mod highlight;
mod highlight_cache;
mod search;
mod split;
#[cfg(test)]
mod tests;

pub(crate) use highlight::highlight_line_segments;
pub use highlight::{DIFF_THEME, HIGHLIGHT_LINE_CAP, HighlightSegment};
pub(crate) use highlight::{
    HighlightCheckpoint, LineHighlighter, LineHighlighterPair, PairCheckpoint,
};
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
    /// built on demand by `ensure_lower_cache`: a load without an active
    /// search skips the lowercase pass, and per-keystroke search only pays it
    /// once per generation.
    pub(crate) hunks_lines_lower: Vec<Vec<String>>,
    /// Cached syntect highlight output per body line. Built once when hunks
    /// (or the active syntax) change so the renderer skips the full-document
    /// state-recovery pass.
    pub line_highlights: Vec<Vec<Vec<HighlightSegment>>>,
    /// Highlight output of the previous generation, kept so a diff that only
    /// appends can move its unchanged hunk prefix into the new cache.
    previous_highlights: Vec<Vec<Vec<HighlightSegment>>>,
    /// Hunks that produced `previous_highlights`, kept for that comparison.
    /// Truncated to the highlighted hunks so a capped diff does not retain
    /// text the next comparison can never reach.
    previous_hunks: Vec<DiffHunk>,
    /// Saved state after each cache entry of the previous pass, aligned with
    /// `previous_highlights`, so highlighting can resume at its end.
    previous_checkpoints: Vec<PairCheckpoint>,
    /// Saved state after each cache entry of the current pass, moved to
    /// `previous_checkpoints` by the next `set_hunks`.
    highlight_checkpoints: Vec<PairCheckpoint>,
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
