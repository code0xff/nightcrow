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
        self.hunks = hunks;
        self.generation = self.generation.wrapping_add(1);

        self.total_lines = 0;
        self.max_line_number = 0;

        self.hunks_lines_lower.clear();
        self.hunks_lines_lower.reserve(self.hunks.len());
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

            let mut lines_lower = Vec::with_capacity(hunk.lines.len());
            for line in &hunk.lines {
                if let Some(old) = line.old_lineno {
                    self.max_line_number = self.max_line_number.max(old);
                }
                if let Some(new) = line.new_lineno {
                    self.max_line_number = self.max_line_number.max(new);
                }
                lines_lower.push(line.content.to_lowercase());
            }
            self.hunks_lines_lower.push(lines_lower);
        }
        self.lower_cache_generation = Some(self.generation);

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

        self.line_highlights.clear();
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
