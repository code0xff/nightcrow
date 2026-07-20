//! Server-side syntax highlighting for the viewer.
//!
//! Reuses `syntect` + `two-face` — already dependencies, and the exact way the
//! TUI highlights — so the browser needs no highlighter of its own and the
//! colours match the terminal UI. Highlighting runs on the request thread; the
//! diff and file byte ceilings in [`super::limits`] bound the work.

use crate::web::viewer::dto::SpanDto;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;

/// Grey fallback for a line syntect refuses to highlight.
const FALLBACK_COLOR: &str = "#c8c8c8";

fn assets() -> &'static (SyntaxSet, Theme) {
    static ASSETS: OnceLock<(SyntaxSet, Theme)> = OnceLock::new();
    ASSETS.get_or_init(|| {
        let ss = two_face::syntax::extra_newlines();
        let ts = ThemeSet::load_defaults();
        // The same theme the TUI uses, so terminal and browser render identical
        // colours (see `ui::diff_pane::DIFF_THEME`).
        let theme = ts
            .themes
            .get("base16-ocean.dark")
            .or_else(|| ts.themes.values().next())
            .cloned()
            .expect("syntect ships default themes");
        (ss, theme)
    })
}

/// A line-by-line highlighter for one syntax. State carries across `line`
/// calls, so multi-line constructs (block comments, strings) stay coherent
/// within a single file or hunk.
pub struct Highlighter {
    hl: HighlightLines<'static>,
    ss: &'static SyntaxSet,
}

impl Highlighter {
    /// Highlight one source line into coloured spans. `raw` must not include a
    /// trailing newline.
    pub fn line(&mut self, raw: &str) -> Vec<SpanDto> {
        // syntect wants a trailing newline to terminate a line; strip it back
        // off the segments so span text matches the source exactly.
        let with_nl = format!("{raw}\n");
        match self.hl.highlight_line(&with_nl, self.ss) {
            Ok(ranges) => ranges
                .into_iter()
                .filter_map(|(style, text)| {
                    let trimmed = text.trim_end_matches('\n');
                    if trimmed.is_empty() {
                        return None;
                    }
                    let fg = style.foreground;
                    Some(SpanDto {
                        t: trimmed.to_string(),
                        c: format!("#{:02x}{:02x}{:02x}", fg.r, fg.g, fg.b),
                    })
                })
                .collect(),
            Err(_) => vec![SpanDto {
                t: raw.to_string(),
                c: FALLBACK_COLOR.to_string(),
            }],
        }
    }
}

/// A highlighter for the syntax inferred from `path`'s extension — plain text
/// when the extension is unknown or absent.
pub fn highlighter(path: Option<&str>) -> Highlighter {
    let (ss, theme) = assets();
    let syntax = path
        .and_then(|p| {
            std::path::Path::new(p)
                .extension()
                .and_then(|e| e.to_str())
        })
        .and_then(|ext| ss.find_syntax_by_extension(ext))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    Highlighter {
        hl: HighlightLines::new(syntax, theme),
        ss,
    }
}

/// Highlight a whole file into per-line spans.
pub fn file_spans(path: &str, content: &str) -> Vec<Vec<SpanDto>> {
    let mut lighter = highlighter(Some(path));
    content.lines().map(|line| lighter.line(line)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_rust_into_multiple_coloured_spans() {
        let lines = file_spans("main.rs", "let x = 1;\n");
        assert_eq!(lines.len(), 1);
        // A keyword, a name, and punctuation should not all be one colour.
        let colours: std::collections::HashSet<_> = lines[0].iter().map(|s| &s.c).collect();
        assert!(colours.len() > 1, "expected multiple colours: {:?}", lines[0]);
        // Every span carries a #rrggbb colour.
        assert!(lines[0].iter().all(|s| s.c.starts_with('#') && s.c.len() == 7));
    }

    #[test]
    fn unknown_extension_still_returns_spans() {
        let lines = file_spans("notes.unknownext", "hello world\n");
        assert_eq!(lines.len(), 1);
        assert!(!lines[0].is_empty());
    }
}
