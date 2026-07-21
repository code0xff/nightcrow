//! The viewer's wire format.
//!
//! Internal git types are never serialized directly. They carry fields that
//! exist only to make the TUI fast — `search_lower`, `summary_lower` — and
//! types like `Oid` whose serde shape is libgit2's business, not a protocol
//! decision. Every payload below is an explicit whitelist built by hand, so
//! adding a field to an internal struct can never widen what a browser sees,
//! and renaming one breaks the build here instead of silently changing the API.
//!
//! [`PROTOCOL_VERSION`] rides on every response. A cached page from an older
//! build can then refuse to interpret a newer payload rather than misread it.

use crate::git::diff::{ChangedFile, CommitEntry, DiffHunk, LineKind, StatusKind, TrackingStatus};
use crate::web::viewer::highlight;
use crate::web::viewer::limits::{self, Capped};
use serde::Serialize;

/// One navigable directory in the "open a project" folder picker. Directories
/// only — files are not openable as projects. `is_repo` flags a git worktree so
/// the picker can mark it.
#[derive(Debug, Serialize)]
pub struct BrowseEntryDto {
    pub name: String,
    pub is_repo: bool,
}

/// One level of the server filesystem for the folder picker. Unlike [`TreeDto`]
/// this is deliberately *not* confined to a worktree — it browses the server to
/// find a repository to open — so it is reachable only authenticated and
/// carries the same trust as the terminal. `parent` is `None` at the root.
#[derive(Debug, Serialize)]
pub struct BrowseDto {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub entries: Vec<BrowseEntryDto>,
    pub truncated: bool,
}

/// Bumped whenever an existing field changes meaning or disappears. Adding a
/// new optional field does not need a bump.
pub const PROTOCOL_VERSION: u32 = 2;

/// Wrapper carried by every response so a stale client can detect a mismatch.
#[derive(Debug, Serialize)]
pub struct Envelope<T> {
    pub version: u32,
    #[serde(flatten)]
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(payload: T) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            payload,
        }
    }
}

/// One open repository. `id` is opaque and stable for the process lifetime;
/// clients address every other route by it and never by path.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoDto {
    pub id: String,
    /// Final path component, for a tab label.
    pub name: String,
    /// Home-relative path for display (`~/code/app`). The absolute path is not
    /// sent: the client never needs it, and it is the one field here that says
    /// something about the machine rather than the repository.
    pub display_path: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrackingDto {
    pub ahead: usize,
    pub behind: usize,
}

impl From<&TrackingStatus> for TrackingDto {
    fn from(t: &TrackingStatus) -> Self {
        Self {
            ahead: t.ahead,
            behind: t.behind,
        }
    }
}

/// One changed file. `index`/`worktree` are the two `git status --short`
/// columns as single-character codes.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ChangedFileDto {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    pub index: String,
    pub worktree: String,
}

/// Wire code for a status column. Defined here rather than reused from the TUI
/// renderer so the protocol does not shift if the display characters do.
fn status_code(kind: StatusKind) -> &'static str {
    match kind {
        StatusKind::Unmodified => " ",
        StatusKind::Added => "A",
        StatusKind::Modified => "M",
        StatusKind::Deleted => "D",
        StatusKind::Renamed => "R",
        StatusKind::TypeChanged => "T",
        StatusKind::Untracked => "?",
        StatusKind::Unmerged => "U",
    }
}

impl From<&ChangedFile> for ChangedFileDto {
    fn from(f: &ChangedFile) -> Self {
        // `search_lower` is deliberately absent: it is a TUI filter cache.
        Self {
            path: f.path.clone(),
            old_path: f.old_path.clone(),
            index: status_code(f.index).to_string(),
            worktree: status_code(f.worktree).to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct StatusDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tracking: Option<TrackingDto>,
    pub files: Vec<ChangedFileDto>,
    /// True when the repository had more changed files than the ceiling.
    pub truncated: bool,
}

impl StatusDto {
    pub fn from_snapshot(
        files: &[ChangedFile],
        tracking: Option<&TrackingStatus>,
        head: Option<git2::Oid>,
        branch: Option<&str>,
    ) -> Self {
        let capped = Capped::new(files.to_vec(), limits::MAX_STATUS_FILES);
        Self {
            branch: branch.map(str::to_string),
            // `Oid`'s own serde shape is libgit2's concern; hex is the protocol's.
            head: head.map(|oid| oid.to_string()),
            tracking: tracking.map(TrackingDto::from),
            files: capped.items.iter().map(ChangedFileDto::from).collect(),
            truncated: capped.truncated,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommitDto {
    pub oid: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    /// Unix seconds. Formatting is the client's business.
    pub time: i64,
}

impl From<&CommitEntry> for CommitDto {
    fn from(c: &CommitEntry) -> Self {
        // `summary_lower` is deliberately absent: it is a TUI filter cache.
        Self {
            oid: c.oid.to_string(),
            short_id: c.short_id.clone(),
            summary: c.summary.clone(),
            author: c.author.clone(),
            time: c.time,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LogDto {
    pub commits: Vec<CommitDto>,
    pub truncated: bool,
}

/// Changed paths in one historical commit. The row shape intentionally matches
/// [`ChangedFileDto`], so the browser renders status and commit drill-down
/// lists consistently (including rename sources and XY-style status columns).
#[derive(Debug, Clone, Serialize)]
pub struct CommitFilesDto {
    pub files: Vec<ChangedFileDto>,
    pub truncated: bool,
}

impl CommitFilesDto {
    pub fn from_entries(files: &[ChangedFile]) -> Self {
        let capped = Capped::new(files.to_vec(), limits::MAX_COMMIT_FILES);
        Self {
            files: capped.items.iter().map(ChangedFileDto::from).collect(),
            truncated: capped.truncated,
        }
    }
}

impl LogDto {
    pub fn from_entries(entries: &[CommitEntry]) -> Self {
        let capped = Capped::new(entries.to_vec(), limits::MAX_LOG_PAGE);
        Self {
            commits: capped.items.iter().map(CommitDto::from).collect(),
            truncated: capped.truncated,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TreeEntryDto {
    pub name: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TreeDto {
    pub path: String,
    pub entries: Vec<TreeEntryDto>,
    pub truncated: bool,
}

impl TreeDto {
    pub fn from_entries(path: &str, entries: &[crate::git::tree::TreeEntry]) -> Self {
        let capped = Capped::new(entries.to_vec(), limits::MAX_TREE_ENTRIES);
        Self {
            path: path.to_string(),
            entries: capped
                .items
                .iter()
                .map(|e| TreeEntryDto {
                    name: e.name.clone(),
                    is_dir: e.is_dir,
                })
                .collect(),
            truncated: capped.truncated,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TreeMatchDto {
    pub path: String,
    pub is_dir: bool,
}

/// Result of a recursive `/api/tree/search`: full paths whose basename matched
/// the query, already sorted and capped by the search walk.
#[derive(Debug, Clone, Serialize)]
pub struct TreeSearchDto {
    pub query: String,
    pub matches: Vec<TreeMatchDto>,
    pub truncated: bool,
}

impl TreeSearchDto {
    pub fn new(query: &str, matches: &[crate::git::tree::TreeMatch], truncated: bool) -> Self {
        Self {
            query: query.to_string(),
            matches: matches
                .iter()
                .map(|m| TreeMatchDto {
                    path: m.path.clone(),
                    is_dir: m.is_dir,
                })
                .collect(),
            truncated,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn json<T: Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value).unwrap()
    }

    #[test]
    fn changed_file_dto_drops_the_tui_search_cache() {
        let file = ChangedFile::from_status_columns(
            "src/main.rs".to_string(),
            None,
            StatusKind::Modified,
            StatusKind::Unmodified,
        );
        assert!(
            !file.search_lower.is_empty(),
            "precondition: the internal type carries a search cache"
        );

        let value = json(&ChangedFileDto::from(&file));

        assert_eq!(value["path"], "src/main.rs");
        assert_eq!(value["index"], "M");
        assert_eq!(value["worktree"], " ");
        assert!(
            value.get("search_lower").is_none(),
            "the filter cache must not reach the wire: {value}"
        );
        assert!(
            value.get("old_path").is_none(),
            "an absent rename source is omitted, not null"
        );
    }

    #[test]
    fn changed_file_dto_keeps_a_rename_source() {
        let file = ChangedFile::from_status_columns(
            "new.rs".to_string(),
            Some("old.rs".to_string()),
            StatusKind::Renamed,
            StatusKind::Unmodified,
        );

        let value = json(&ChangedFileDto::from(&file));

        assert_eq!(value["old_path"], "old.rs");
        assert_eq!(value["index"], "R");
    }

    #[test]
    fn commit_dto_drops_the_summary_cache_and_hexes_the_oid() {
        let entry = CommitEntry::new(
            git2::Oid::from_str("1234567890abcdef1234567890abcdef12345678").unwrap(),
            "1234567".to_string(),
            "Fix The Bug".to_string(),
            "Someone".to_string(),
            1_700_000_000,
        );
        assert!(!entry.summary_lower.is_empty(), "precondition");

        let value = json(&CommitDto::from(&entry));

        assert_eq!(value["oid"], "1234567890abcdef1234567890abcdef12345678");
        assert_eq!(value["summary"], "Fix The Bug");
        assert!(
            value.get("summary_lower").is_none(),
            "the filter cache must not reach the wire: {value}"
        );
    }

    #[test]
    fn envelope_carries_the_protocol_version_alongside_the_payload() {
        let value = json(&Envelope::new(TreeDto::from_entries("src", &[])));

        assert_eq!(value["version"], PROTOCOL_VERSION);
        assert_eq!(value["path"], "src", "the payload is flattened, not nested");
    }

    #[test]
    fn status_dto_reports_truncation_past_the_ceiling() {
        let files: Vec<_> = (0..limits::MAX_STATUS_FILES + 5)
            .map(|i| {
                ChangedFile::from_status_columns(
                    format!("f{i}.rs"),
                    None,
                    StatusKind::Modified,
                    StatusKind::Unmodified,
                )
            })
            .collect();

        let dto = StatusDto::from_snapshot(&files, None, None, Some("main"));

        assert_eq!(dto.files.len(), limits::MAX_STATUS_FILES);
        assert!(dto.truncated);
        assert_eq!(dto.branch.as_deref(), Some("main"));
    }

    #[test]
    fn status_dto_omits_absent_optional_fields() {
        let value = json(&StatusDto::from_snapshot(&[], None, None, None));

        assert!(value.get("branch").is_none());
        assert!(value.get("head").is_none());
        assert!(value.get("tracking").is_none());
        assert_eq!(value["truncated"], false);
    }

    fn hunk(header: &str, lines: usize, width: usize) -> DiffHunk {
        DiffHunk {
            header: header.to_string(),
            file_path: None,
            lines: (0..lines)
                .map(|_| crate::git::diff::DiffLine {
                    kind: LineKind::Context,
                    content: "x".repeat(width),
                })
                .collect(),
        }
    }

    #[test]
    fn diff_dto_maps_line_kinds_to_wire_codes() {
        let hunks = vec![DiffHunk {
            header: "@@ -1 +1 @@".to_string(),
            file_path: Some("a.rs".to_string()),
            lines: vec![
                crate::git::diff::DiffLine {
                    kind: LineKind::Added,
                    content: "new".into(),
                },
                crate::git::diff::DiffLine {
                    kind: LineKind::Removed,
                    content: "old".into(),
                },
                crate::git::diff::DiffLine {
                    kind: LineKind::Context,
                    content: "same".into(),
                },
            ],
        }];

        let dto = DiffDto::from_hunks("a.rs", &hunks);

        let kinds: Vec<_> = dto.hunks[0].lines.iter().map(|l| l.kind.as_str()).collect();
        assert_eq!(kinds, vec!["+", "-", " "]);
        assert!(!dto.truncated);
    }

    #[test]
    fn diff_dto_caps_across_hunks_not_within_one() {
        // Each hunk is under the ceiling alone; together they exceed it. A
        // per-hunk cap would let the total through unbounded.
        let per_hunk = limits::MAX_DIFF_LINES / 2 + 10;
        let hunks = vec![hunk("@@ a @@", per_hunk, 1), hunk("@@ b @@", per_hunk, 1)];

        let dto = DiffDto::from_hunks("big.rs", &hunks);

        let total: usize = dto.hunks.iter().map(|h| h.lines.len()).sum();
        assert_eq!(total, limits::MAX_DIFF_LINES);
        assert!(dto.truncated);
    }

    #[test]
    fn diff_dto_stops_on_the_byte_ceiling_before_the_line_ceiling() {
        // Few lines, each enormous: the byte ceiling has to bind first.
        let hunks = vec![hunk("@@ a @@", 50, limits::MAX_DIFF_BYTES / 10)];

        let dto = DiffDto::from_hunks("wide.rs", &hunks);

        let bytes: usize = dto.hunks[0]
            .lines
            .iter()
            .flat_map(|l| &l.spans)
            .map(|s| s.t.len())
            .sum();
        assert!(bytes <= limits::MAX_DIFF_BYTES);
        assert!(dto.truncated);
        assert!(
            dto.hunks[0].lines.len() < 50,
            "the byte ceiling must cut before the line count does"
        );
    }

    #[test]
    fn file_dto_caps_content_on_a_character_boundary() {
        let content = "한".repeat(limits::MAX_DIFF_BYTES);

        let dto = FileDto::new("big.txt", &content);

        // Reconstruct the served text from its spans (one line here — no \n).
        let served: String = dto.lines.iter().flatten().map(|s| s.t.as_str()).collect();
        assert!(dto.truncated);
        assert!(served.len() <= limits::MAX_DIFF_BYTES);
        assert!(
            content.starts_with(&served),
            "the cap must yield a clean prefix"
        );
    }
}
