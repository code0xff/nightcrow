use super::status::ChangedFileDto;
use crate::git::diff::{ChangedFile, CommitEntry, LogDecorations, RefKind};
use crate::web::viewer::limits::{self, Capped};
use git2::Oid;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommitDto {
    pub oid: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    /// Unix seconds. Formatting is the client's business.
    pub time: i64,
    /// Refs pointing at this commit, most orienting first (HEAD, local
    /// branches, tags, remote branches) — the order the TUI's chips use.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refs: Vec<RefDto>,
    /// `"ahead"` of the upstream or `"behind"` it; absent when neither.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub divergence: Option<&'static str>,
    #[serde(skip_serializing_if = "is_false")]
    pub merge: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RefDto {
    /// `head`, `local`, `tag` or `remote`.
    pub kind: &'static str,
    /// Shorthand: `dev`, `origin/dev`, `v0.1.15`. `HEAD` for a detached head.
    pub name: String,
}

impl CommitDto {
    /// One log row, decorated the way the TUI's commit list is.
    pub fn decorated(c: &CommitEntry, decorations: &LogDecorations) -> Self {
        // `summary_lower` is deliberately absent: it is a TUI filter cache.
        Self {
            oid: c.oid.to_string(),
            short_id: c.short_id.clone(),
            summary: c.summary.clone(),
            author: c.author.clone(),
            time: c.time,
            refs: decorations
                .labels_for(c.oid)
                .iter()
                .map(|label| RefDto {
                    kind: match label.kind {
                        RefKind::Head => "head",
                        RefKind::LocalBranch => "local",
                        RefKind::Tag => "tag",
                        RefKind::RemoteBranch => "remote",
                    },
                    name: label.name.clone(),
                })
                .collect(),
            divergence: if decorations.is_ahead(c.oid) {
                Some("ahead")
            } else if decorations.is_behind(c.oid) {
                Some("behind")
            } else {
                None
            },
            merge: c.is_merge(),
        }
    }
}

/// One page of the commit log.
#[derive(Debug, Clone, Serialize)]
pub struct LogDto {
    pub commits: Vec<CommitDto>,
    /// True when the history continues past this page — i.e. there is a next
    /// page to ask for, not that anything was silently dropped.
    pub truncated: bool,
    /// The commit the walk started from, echoed so the client can pin its
    /// following pages to it (see [`crate::git::diff::load_commit_log_from`]).
    /// `None` only for a repository with no commits to anchor to.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
}

/// Changed paths in one historical commit. The row shape intentionally
/// matches [`ChangedFileDto`], so the browser renders status and commit
/// drill-down lists consistently (including rename sources and XY-style
/// status columns).
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
    /// Build a page from a walk that was asked for one entry more than
    /// [`limits::MAX_LOG_PAGE`].
    ///
    /// The extra entry is how "there is more" is known: asking for exactly a
    /// page's worth and capping at the same number can never report truncation,
    /// which is what this endpoint used to do — it answered `truncated: false`
    /// for a history of any length.
    ///
    /// `anchor` is the commit the walk started from, echoed to the client so
    /// its next request describes the same history.
    pub fn from_entries(
        entries: &[CommitEntry],
        anchor: Option<Oid>,
        decorations: &LogDecorations,
    ) -> Self {
        let capped = Capped::new(entries.to_vec(), limits::MAX_LOG_PAGE);
        Self {
            commits: capped
                .items
                .iter()
                .map(|c| CommitDto::decorated(c, decorations))
                .collect(),
            truncated: capped.truncated,
            head: anchor.map(|oid| oid.to_string()),
        }
    }
}
