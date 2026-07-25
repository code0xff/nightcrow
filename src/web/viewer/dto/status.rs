use crate::git::diff::{ChangedFile, StatusKind, TrackingStatus};
use crate::web::viewer::limits::{self, Capped};
use serde::Serialize;
use std::collections::HashMap;
use std::time::SystemTime;

/// One navigable directory in the "open a project" folder picker.
/// Directories only — files are not openable as projects. `is_repo` flags a
/// git worktree so the picker can mark it.
#[derive(Debug, Serialize)]
pub struct BrowseEntryDto {
    pub name: String,
    pub is_repo: bool,
}

/// One level of the server filesystem for the folder picker. Unlike
/// [`super::TreeDto`] this is deliberately *not* confined to a worktree — it
/// browses the server to find a repository to open — so it is reachable only
/// authenticated and carries the same trust as the terminal. `parent` is `None`
/// at the root.
#[derive(Debug, Serialize)]
pub struct BrowseDto {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub entries: Vec<BrowseEntryDto>,
    pub truncated: bool,
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
    /// Worktree mtime as Unix milliseconds, for the client's "recently touched"
    /// highlight (the same signal the TUI's hot table carries). Absent when the
    /// file could not be stat'd — or always, for a commit's file list, where the
    /// working tree says nothing about the commit.
    ///
    /// An absolute instant, not an age: the status payload is deduplicated by
    /// byteequality before it is pushed, so a field that moved every tick would
    /// turn an idle repository into a permanent event stream. Because the
    /// instant comes from this machine's clock and the browser may be running on
    /// another device, the client corrects for the difference using the
    /// `now_ms` that rides the repo poll (see [`server_now_millis`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<u64>,
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
            mtime: None,
        }
    }
}

/// Unix milliseconds, or `None` for a pre-epoch timestamp — which only a badly
/// skewed clock produces, and which the client would read as "infinitely old"
/// anyway.
fn unix_millis(t: SystemTime) -> Option<u64> {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// The server's wall clock in Unix milliseconds — the reference the client dates
/// `mtime` against. `0` for a pre-epoch clock, which leaves the client on its own
/// clock rather than shifting it by a nonsense offset.
///
/// Sent because `mtime` is an absolute instant produced by *this* machine while
/// the browser reading it may be another device entirely (see [`ChangedFile`]).
pub fn server_now_millis() -> u64 {
    unix_millis(SystemTime::now()).unwrap_or(0)
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
    /// `mtimes` is the snapshot worker's stat of every listed file, keyed by
    /// path; paths missing from it simply carry no `mtime`.
    pub fn from_snapshot(
        files: &[ChangedFile],
        tracking: Option<&TrackingStatus>,
        head: Option<git2::Oid>,
        branch: Option<&str>,
        mtimes: &HashMap<String, SystemTime>,
    ) -> Self {
        let capped = Capped::new(files.to_vec(), limits::MAX_STATUS_FILES);
        Self {
            branch: branch.map(str::to_string),
            // `Oid`'s own serde shape is libgit2's concern; hex is the protocol's.
            head: head.map(|oid| oid.to_string()),
            tracking: tracking.map(TrackingDto::from),
            files: capped
                .items
                .iter()
                .map(|f| ChangedFileDto {
                    mtime: mtimes.get(&f.path).copied().and_then(unix_millis),
                    ..ChangedFileDto::from(f)
                })
                .collect(),
            truncated: capped.truncated,
        }
    }
}
