use crate::git::diff::{DiffHunk, LineKind};

use super::{DiffPane, SplitRow, flush_split_blocks};

impl DiffPane {
    /// Borrow the loaded diff without exposing a mutation path that can leave
    /// the derived render indexes stale.
    pub(crate) fn hunks(&self) -> &[DiffHunk] {
        &self.hunks
    }

    /// Replace the loaded diff and rebuild every index derived from it at the
    /// mutation boundary. View, search query, file overlay, anchor, and scroll
    /// are deliberately left untouched; callers decide which of those should
    /// reset for a particular load mode.
    pub fn set_hunks(&mut self, hunks: Vec<DiffHunk>) {
        // Keep the generation that just left the screen so `ensure_highlight_cache`
        // can reuse a shared hunk prefix. An emptied diff has nothing to compare
        // against later, so it releases that memory instead of holding it.
        self.previous_hunks = std::mem::take(&mut self.hunks);
        self.previous_highlights = std::mem::take(&mut self.line_highlights);
        self.previous_checkpoints = std::mem::take(&mut self.highlight_checkpoints);
        self.previous_hunks.truncate(self.previous_highlights.len());
        if hunks.is_empty() {
            self.previous_hunks.clear();
            self.previous_highlights.clear();
            self.previous_checkpoints.clear();
        }

        self.hunks = hunks;
        self.generation = self.generation.wrapping_add(1);

        self.total_lines = 0;
        self.max_line_number = 0;

        self.hunks_lines_lower.clear();
        // Built on demand by `ensure_lower_cache`, so a load without an active
        // search never lowercases the diff.
        self.lower_cache_generation = None;
        self.hunk_starts.clear();
        self.hunk_starts.reserve(self.hunks.len());
        self.syntax_shape.clear();
        self.syntax_shape.reserve(self.hunks.len());
        for hunk in &self.hunks {
            self.hunk_starts.push(self.total_lines);
            self.total_lines = self
                .total_lines
                .saturating_add(1usize.saturating_add(hunk.lines.len()));
            self.syntax_shape.push(
                hunk.file_path
                    .as_deref()
                    .map(crate::ui::path_extension)
                    .map(str::to_owned),
            );

            for line in &hunk.lines {
                if let Some(old) = line.old_lineno {
                    self.max_line_number = self.max_line_number.max(old);
                }
                if let Some(new) = line.new_lineno {
                    self.max_line_number = self.max_line_number.max(new);
                }
            }
        }

        self.split_rows.clear();
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for (hi, hunk) in self.hunks.iter().enumerate() {
            self.split_rows.push(SplitRow::Header(hi));
            for (li, line) in hunk.lines.iter().enumerate() {
                match line.kind {
                    LineKind::Removed => removed.push(li),
                    LineKind::Added => added.push(li),
                    LineKind::Context => {
                        flush_split_blocks(&mut self.split_rows, hi, &mut removed, &mut added);
                        self.split_rows.push(SplitRow::Body {
                            left: Some((hi, li)),
                            right: Some((hi, li)),
                        });
                    }
                }
            }
            flush_split_blocks(&mut self.split_rows, hi, &mut removed, &mut added);
        }

        self.highlight_cache_generation = None;
        self.search.matches.clear();
    }

    #[cfg(test)]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn max_line_number(&self) -> u32 {
        self.max_line_number
    }

    pub(crate) fn hunk_starts(&self) -> &[usize] {
        &self.hunk_starts
    }

    /// Total flat row count across all hunks (1 header + N body lines each).
    pub fn line_count(&self) -> usize {
        self.total_lines
    }

    /// Largest legal `scroll` value: one less than the total row count, or 0
    /// when there are no rows.
    pub fn max_scroll(&self) -> usize {
        self.line_count().saturating_sub(1)
    }

    /// Borrow the mutation-time side-by-side row layout. Within each hunk,
    /// consecutive removed/added lines are paired index-by-index (the shorter
    /// run padded with blank cells) and context lines are mirrored.
    pub fn split_rows(&self) -> &[SplitRow] {
        &self.split_rows
    }
}
