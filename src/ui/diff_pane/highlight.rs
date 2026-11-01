use syntect::highlighting::{HighlightIterator, HighlightState, Highlighter, Theme};
use syntect::parsing::{ParseState, ScopeStack, SyntaxReference, SyntaxSet};

/// Syntect theme name used for both the diff and file-view highlight caches.
pub const DIFF_THEME: &str = "base16-ocean.dark";

/// Leading body lines highlighted per loaded diff or file overlay. Past this,
/// renderers fall back to plain text: a large or constantly appended file would
/// otherwise spend a full syntect pass on the UI thread every time its
/// generation changes. Diff add/remove backgrounds come from the line kind, so
/// they still apply past the cap.
pub const HIGHLIGHT_LINE_CAP: usize = 4000;

/// One highlighted segment of a body line: foreground RGB + the text, cached
/// so per-frame rendering does not re-run the syntect highlighter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightSegment {
    pub rgb: (u8, u8, u8),
    pub text: String,
}

/// Parser and highlighter state at one line boundary. Resuming from a
/// checkpoint produces the same segments a full pass would, without replaying
/// the lines that produced the state.
#[derive(Debug, Clone)]
pub(crate) struct HighlightCheckpoint {
    parse_state: ParseState,
    highlight_state: HighlightState,
}

/// One syntect highlighter whose state can be saved and restored.
///
/// `syntect::easy::HighlightLines` wraps the same pair but exposes its state
/// only by consuming itself, so it cannot checkpoint while continuing.
pub(crate) struct LineHighlighter<'a> {
    highlighter: Highlighter<'a>,
    parse_state: ParseState,
    highlight_state: HighlightState,
}

impl<'a> LineHighlighter<'a> {
    pub(crate) fn new(syntax: &SyntaxReference, theme: &'a Theme) -> Self {
        let highlighter = Highlighter::new(theme);
        let highlight_state = HighlightState::new(&highlighter, ScopeStack::new());
        Self {
            highlighter,
            parse_state: ParseState::new(syntax),
            highlight_state,
        }
    }

    pub(crate) fn resume(theme: &'a Theme, checkpoint: HighlightCheckpoint) -> Self {
        Self {
            highlighter: Highlighter::new(theme),
            parse_state: checkpoint.parse_state,
            highlight_state: checkpoint.highlight_state,
        }
    }

    pub(crate) fn checkpoint(&self) -> HighlightCheckpoint {
        HighlightCheckpoint {
            parse_state: self.parse_state.clone(),
            highlight_state: self.highlight_state.clone(),
        }
    }

    fn highlight_line(&mut self, ss: &SyntaxSet, raw: &str) -> Vec<HighlightSegment> {
        // syntect expects trailing newlines to terminate lines; strip them back
        // off the resulting segments so cached text matches the source line.
        let with_nl = format!("{raw}\n");
        let ops = match self.parse_state.parse_line(&with_nl, ss) {
            Ok(ops) => ops,
            Err(_) => {
                return vec![HighlightSegment {
                    rgb: (200, 200, 200),
                    text: raw.to_string(),
                }];
            }
        };
        HighlightIterator::new(&mut self.highlight_state, &ops, &with_nl, &self.highlighter)
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
            .collect()
    }
}

/// Saved new-side and old-side highlighter states at one diff hunk boundary.
#[derive(Debug, Clone)]
pub(crate) struct PairCheckpoint {
    /// Syntax the pair was highlighting at this boundary; a different syntax
    /// at the same boundary makes the full pass reset both sides.
    pub(crate) syntax_name: String,
    pub(crate) new_side: HighlightCheckpoint,
    pub(crate) old_side: HighlightCheckpoint,
}

/// The highlighter pair a diff keeps in lockstep: removed lines feed the
/// old-side state, every other line feeds the new-side state.
pub(crate) struct LineHighlighterPair<'a> {
    new_side: LineHighlighter<'a>,
    old_side: LineHighlighter<'a>,
}

impl<'a> LineHighlighterPair<'a> {
    pub(crate) fn new(syntax: &SyntaxReference, theme: &'a Theme) -> Self {
        Self {
            new_side: LineHighlighter::new(syntax, theme),
            old_side: LineHighlighter::new(syntax, theme),
        }
    }

    pub(crate) fn resume(theme: &'a Theme, checkpoint: PairCheckpoint) -> Self {
        Self {
            new_side: LineHighlighter::resume(theme, checkpoint.new_side),
            old_side: LineHighlighter::resume(theme, checkpoint.old_side),
        }
    }

    pub(crate) fn checkpoint(&self, syntax_name: &str) -> PairCheckpoint {
        PairCheckpoint {
            syntax_name: syntax_name.to_owned(),
            new_side: self.new_side.checkpoint(),
            old_side: self.old_side.checkpoint(),
        }
    }

    pub(crate) fn side_mut(&mut self, removed: bool) -> &mut LineHighlighter<'a> {
        if removed {
            &mut self.old_side
        } else {
            &mut self.new_side
        }
    }
}

/// Run a single line through the supplied highlighter and convert the result
/// into `HighlightSegment`s, falling back to a single grey segment on error.
/// Shared by `DiffPane` and `FileViewState` so both caches build segments
/// identically.
pub(crate) fn highlight_line_segments(
    hl: &mut LineHighlighter<'_>,
    ss: &SyntaxSet,
    raw: &str,
) -> Vec<HighlightSegment> {
    hl.highlight_line(ss, raw)
}
