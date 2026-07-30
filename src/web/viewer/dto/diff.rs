use crate::git::diff::{DiffHunk, LineKind};
use crate::web::viewer::highlight;
use crate::web::viewer::limits;
use serde::Serialize;

/// One run of characters sharing a colour, from server-side syntax
/// highlighting. `t` is the text, `c` a `#rrggbb` foreground.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SpanDto {
    pub t: String,
    pub c: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiffLineDto {
    /// `+`, `-`, or ` `.
    pub kind: String,
    /// Syntax-highlighted content as coloured spans.
    pub spans: Vec<SpanDto>,
    /// Line number on the pre-image side, absent on an added line (which
    /// exists only on the new side) — the client leaves that column blank
    /// rather than deriving a number the line does not have.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_lineno: Option<u32>,
    /// Line number on the post-image side, absent on a removed line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_lineno: Option<u32>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiffHunkDto {
    pub header: String,
    /// Which file the hunk belongs to. Present on commit diffs, where one
    /// response spans several files; absent on a single-file diff.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    pub lines: Vec<DiffLineDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiffDto {
    pub path: String,
    pub hunks: Vec<DiffHunkDto>,
    pub truncated: bool,
}

fn line_code(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Added => "+",
        LineKind::Removed => "-",
        LineKind::Context => " ",
    }
}

impl DiffDto {
    /// Build from loaded hunks, enforcing the diff ceilings across the whole
    /// file rather than per hunk — the cost to a client is the total, and a
    /// pathological diff is usually many hunks rather than one huge one.
    pub fn from_hunks(path: &str, hunks: &[DiffHunk]) -> Self {
        let mut out = Vec::new();
        let mut lines_used = 0usize;
        let mut bytes_used = 0usize;
        let mut truncated = false;

        'outer: for hunk in hunks {
            // One highlighter per hunk, using the hunk's own file on commit
            // diffs (which span several files) and the request path otherwise.
            let mut lighter =
                highlight::highlighter(Some(hunk.file_path.as_deref().unwrap_or(path)));
            let mut kept = Vec::new();
            for line in &hunk.lines {
                if lines_used >= limits::MAX_DIFF_LINES
                    || bytes_used + line.content.len() > limits::MAX_DIFF_BYTES
                {
                    truncated = true;
                    if !kept.is_empty() {
                        out.push(DiffHunkDto {
                            header: hunk.header.clone(),
                            file_path: hunk.file_path.clone(),
                            lines: kept,
                        });
                    }
                    break 'outer;
                }
                lines_used += 1;
                bytes_used += line.content.len();
                kept.push(DiffLineDto {
                    kind: line_code(line.kind).to_string(),
                    spans: lighter.line(&line.content),
                    old_lineno: line.old_lineno,
                    new_lineno: line.new_lineno,
                });
            }
            out.push(DiffHunkDto {
                header: hunk.header.clone(),
                file_path: hunk.file_path.clone(),
                lines: kept,
            });
        }

        Self {
            path: path.to_string(),
            hunks: out,
            truncated,
        }
    }
}

/// A file's syntax-highlighted content, already capped. One entry per line,
/// each a list of coloured spans.
#[derive(Debug, Clone, Serialize)]
pub struct FileDto {
    pub path: String,
    pub lines: Vec<Vec<SpanDto>>,
    pub truncated: bool,
}

impl FileDto {
    pub fn new(path: &str, content: &str) -> Self {
        let (content, truncated) = limits::cap_text(content, limits::MAX_DIFF_BYTES);
        Self {
            path: path.to_string(),
            lines: highlight::file_spans(path, &content),
            truncated,
        }
    }
}
