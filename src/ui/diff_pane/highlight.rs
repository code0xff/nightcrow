/// Syntect theme name used for both the diff and file-view highlight caches.
pub const DIFF_THEME: &str = "base16-ocean.dark";

/// One highlighted segment of a body line: foreground RGB + the text.
/// Cached so per-frame rendering does not re-run the syntect highlighter
/// over the whole document for state recovery.
#[derive(Debug, Clone)]
pub struct HighlightSegment {
    pub rgb: (u8, u8, u8),
    pub text: String,
}

/// Run a single line through the supplied syntect highlighter and convert
/// the result into `HighlightSegment`s. Falls back to a single grey segment
/// on highlighter error. Shared by `DiffPane` and `FileViewState` so both
/// caches build segments identically.
pub(crate) fn highlight_line_segments(
    hl: &mut syntect::easy::HighlightLines,
    ss: &syntect::parsing::SyntaxSet,
    raw: &str,
) -> Vec<HighlightSegment> {
    // syntect expects trailing newlines to terminate lines; strip them back
    // off the resulting segments so cached text matches the source line.
    let with_nl = format!("{raw}\n");
    match hl.highlight_line(&with_nl, ss) {
        Ok(ranges) => ranges
            .into_iter()
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
            .collect(),
        Err(_) => vec![HighlightSegment {
            rgb: (200, 200, 200),
            text: raw.to_string(),
        }],
    }
}
