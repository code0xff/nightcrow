use crate::git::diff::LineKind;
use crate::ui::diff_pane::{
    DIFF_THEME, DiffPane, HIGHLIGHT_LINE_CAP, HighlightSegment, LineHighlighterPair,
    PairCheckpoint, highlight_line_segments, resolve_syntax_extension,
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

/// The reusable share of the previous highlight cache.
struct ReusedPrefix {
    /// Leading hunks whose whole cache entry is still valid.
    whole_hunks: usize,
    /// When the first changed hunk merely grew, the number of leading lines of
    /// its cache entry that are still valid.
    partial_lines: Option<usize>,
}

impl DiffPane {
    /// Ensure `line_highlights` matches the current generation, resolving the
    /// syntax separately for each hunk: a commit diff can touch files of
    /// different types, and a single syntax would render everything as the
    /// first file's language. The generation check is the frame hot path; the
    /// syntax/highlight walk happens only after `set_hunks`.
    ///
    /// The unchanged leading hunks of the previous generation are moved into
    /// the new cache and the pass resumes from the saved state at the first
    /// changed line, so an appended file only pays for its new lines. Any
    /// change to a hunk's lines, kinds, or syntax stops the reuse there; the
    /// rest is recomputed exactly as a full pass would. Headers do not affect
    /// segments, so a changed range on an appended hunk still reuses its lines.
    ///
    /// Only the first [`HIGHLIGHT_LINE_CAP`] body lines are highlighted. A
    /// hunk past the cap gets no entry and a hunk straddling it keeps only its
    /// in-budget prefix, so the renderers' `(hunk, line)` cache lookup falls
    /// back to plain text for everything beyond.
    pub fn ensure_highlight_cache(&mut self, ss: &SyntaxSet, ts: &ThemeSet) {
        if self.highlight_cache_generation == Some(self.generation) {
            return;
        }

        let theme = &ts.themes[DIFF_THEME];
        let mut out: Vec<Vec<Vec<HighlightSegment>>> = Vec::with_capacity(self.hunks.len());
        let mut checkpoints: Vec<PairCheckpoint> = Vec::with_capacity(self.hunks.len());
        let mut hl_pair: Option<LineHighlighterPair<'_>> = None;
        let mut current_syntax_name = String::new();

        let reused = self.reusable_prefix(ss);
        let partial = reused.partial_lines.map(|lines| {
            (
                lines,
                std::mem::take(&mut self.previous_highlights[reused.whole_hunks]),
                self.previous_checkpoints[reused.whole_hunks].clone(),
            )
        });
        out.extend(self.previous_highlights.drain(..reused.whole_hunks));
        checkpoints.extend(self.previous_checkpoints.drain(..reused.whole_hunks));
        let mut remaining = HIGHLIGHT_LINE_CAP.saturating_sub(out.iter().map(Vec::len).sum());
        let mut first_hunk = reused.whole_hunks;

        if let Some((shared, mut entry, checkpoint)) = partial {
            // The reused lines already consumed this much of the cap.
            remaining = remaining.saturating_sub(shared);
            let hunk = &self.hunks[first_hunk];
            let syntax = self.syntax_at(ss, first_hunk);
            hl_pair = Some(LineHighlighterPair::resume(theme, checkpoint));
            current_syntax_name = syntax.name.clone();
            let pair = hl_pair.as_mut().unwrap();
            let additional = (hunk.lines.len() - shared).min(remaining);
            for line in hunk.lines.iter().skip(shared).take(additional) {
                let hl = pair.side_mut(matches!(line.kind, LineKind::Removed));
                entry.push(highlight_line_segments(hl, ss, &line.content));
            }
            remaining -= additional;
            out.push(entry);
            checkpoints.push(pair.checkpoint(current_syntax_name.as_str()));
            first_hunk += 1;
        } else if remaining > 0 && first_hunk < self.hunks.len() {
            // Resume at the first changed hunk. A syntax change there makes the
            // full pass reset both sides, so the previous syntax's states must
            // not be carried over.
            let boundary = self.syntax_at(ss, first_hunk);
            if first_hunk > 0
                && let Some(checkpoint) = checkpoints.last()
                && checkpoint.syntax_name == boundary.name
            {
                hl_pair = Some(LineHighlighterPair::resume(theme, checkpoint.clone()));
                current_syntax_name = boundary.name.clone();
            }
        }

        for idx in first_hunk..self.hunks.len() {
            if remaining == 0 {
                break;
            }
            let hunk = &self.hunks[idx];
            let syntax = self.syntax_at(ss, idx);
            if hl_pair.is_none() || current_syntax_name != syntax.name {
                hl_pair = Some(LineHighlighterPair::new(syntax, theme));
                current_syntax_name = syntax.name.clone();
            }
            // Safe: assigned above when it was None.
            let pair = hl_pair.as_mut().unwrap();

            let take = hunk.lines.len().min(remaining);
            let mut per_hunk: Vec<Vec<HighlightSegment>> = Vec::with_capacity(take);
            for line in hunk.lines.iter().take(take) {
                let hl = pair.side_mut(matches!(line.kind, LineKind::Removed));
                per_hunk.push(highlight_line_segments(hl, ss, &line.content));
            }
            remaining -= take;
            out.push(per_hunk);
            checkpoints.push(pair.checkpoint(current_syntax_name.as_str()));
        }

        self.line_highlights = out;
        self.highlight_checkpoints = checkpoints;
        self.highlight_cache_generation = Some(self.generation);
        self.previous_hunks.clear();
        self.previous_highlights.clear();
        self.previous_checkpoints.clear();
    }

    fn syntax_at<'a>(
        &self,
        ss: &'a SyntaxSet,
        idx: usize,
    ) -> &'a syntect::parsing::SyntaxReference {
        resolve_syntax_extension(
            ss,
            self.syntax_shape
                .get(idx)
                .and_then(|extension| extension.as_deref()),
        )
    }

    /// Longest prefix of the previous cache the current hunks still produce.
    ///
    /// Alignment is per hunk because syntax and the cap are resolved per hunk.
    /// A whole entry is reusable when it covers all of the new hunk; when the
    /// first changed hunk only grew, its leading lines remain reusable and the
    /// saved state at their end resumes the pass.
    fn reusable_prefix(&self, ss: &SyntaxSet) -> ReusedPrefix {
        let limit = self
            .hunks
            .len()
            .min(self.previous_hunks.len())
            .min(self.previous_highlights.len())
            .min(self.previous_checkpoints.len());
        let mut whole_hunks = 0;
        while whole_hunks < limit {
            let hunk = &self.hunks[whole_hunks];
            let previous = &self.previous_hunks[whole_hunks];
            let entry = &self.previous_highlights[whole_hunks];
            let syntax = self.syntax_at(ss, whole_hunks);
            // The hunk header only feeds the gutter and title, so it does not
            // affect the cached segments; a changed hunk range on an appended
            // file must still reuse the shared lines.
            if self.previous_checkpoints[whole_hunks].syntax_name != syntax.name {
                break;
            }
            let shared = entry.len().min(hunk.lines.len());
            let same = hunk
                .lines
                .iter()
                .take(shared)
                .zip(previous.lines.iter().take(shared))
                .all(|(line, old)| {
                    line.content == old.content
                        && matches!(line.kind, LineKind::Removed)
                            == matches!(old.kind, LineKind::Removed)
                });
            if !same {
                break;
            }
            if entry.len() == hunk.lines.len() {
                whole_hunks += 1;
                continue;
            }
            if entry.len() < hunk.lines.len() {
                return ReusedPrefix {
                    whole_hunks,
                    partial_lines: Some(entry.len()),
                };
            }
            break;
        }
        ReusedPrefix {
            whole_hunks,
            partial_lines: None,
        }
    }
}
