use super::status::ChangedFileDto;
use crate::git::diff::{ChangedFile, CommitEntry, LogDecorations, RefKind, RefLabel};
use crate::web::viewer::limits::{self, Capped};
use git2::Oid;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CommitDto {
    pub oid: String,
    pub short_id: String,
    pub summary: String,
    pub author: String,
    /// Unix seconds. Formatting is the client's business.
    pub time: i64,
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

impl From<&CommitEntry> for CommitDto {
    fn from(c: &CommitEntry) -> Self {
        // `summary_lower` is deliberately absent: it is a TUI filter cache.
        Self {
            oid: c.oid.to_string(),
            short_id: c.short_id.clone(),
            summary: c.summary.clone(),
            author: c.author.clone(),
            time: c.time,
            merge: c.is_merge(),
        }
    }
}

/// Which refs point at which commits, and which commits stand ahead of or
/// behind the upstream — for the whole repository, not for one page.
///
/// Apart from the log pages on purpose. The history a page describes does not
/// change once walked, but its decorations do whenever a ref moves: a push,
/// a fetch, a branch switch at the same commit. Carried on each row, they went
/// stale on every row already loaded; here the client replaces them wholesale.
#[derive(Debug, Clone, Serialize)]
pub struct LogDecorationsDto {
    /// Oid → labels, most orienting first (HEAD, local, tag, remote).
    pub refs: BTreeMap<String, Vec<RefDto>>,
    /// Commits on this branch the upstream lacks, capped by the walk.
    pub ahead: Vec<String>,
    /// Commits on the upstream this branch lacks, capped by the walk.
    pub behind: Vec<String>,
    /// True when `refs` was cut at [`limits::MAX_LOG_DECORATION_REFS`]. The
    /// cut drops remote branches before tags before local ones, and never the
    /// branch HEAD is on.
    pub truncated: bool,
}

impl From<&LogDecorations> for LogDecorationsDto {
    fn from(d: &LogDecorations) -> Self {
        Self::capped(d, limits::MAX_LOG_DECORATION_REFS)
    }
}

impl LogDecorationsDto {
    /// Every label the repository has, or the `cap` most orienting of them.
    pub fn capped(d: &LogDecorations, cap: usize) -> Self {
        // Flattened and ranked across the whole repository, so a cut keeps
        // HEAD and local branches wherever they are and gives up the long tail
        // of remote branches and release tags first.
        let mut all: Vec<(&Oid, &RefLabel)> = d
            .all_labels()
            .flat_map(|(oid, labels)| labels.iter().map(move |label| (oid, label)))
            .collect();
        all.sort_by(|a, b| {
            a.1.kind
                .cmp(&b.1.kind)
                .then_with(|| a.1.name.cmp(&b.1.name))
        });
        let truncated = all.len() > cap;
        all.truncate(cap);

        let mut refs: BTreeMap<String, Vec<RefDto>> = BTreeMap::new();
        for (oid, label) in all {
            refs.entry(oid.to_string()).or_default().push(RefDto {
                kind: match label.kind {
                    RefKind::Head => "head",
                    RefKind::LocalBranch => "local",
                    RefKind::Tag => "tag",
                    RefKind::RemoteBranch => "remote",
                },
                name: label.name.clone(),
            });
        }
        // Sorted so the payload is the same bytes for the same refs.
        fn sorted<'a>(oids: impl Iterator<Item = &'a Oid>) -> Vec<String> {
            let mut out: Vec<String> = oids.map(Oid::to_string).collect();
            out.sort();
            out
        }
        Self {
            refs,
            ahead: sorted(d.ahead_oids()),
            behind: sorted(d.behind_oids()),
            truncated,
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
    pub fn from_entries(entries: &[CommitEntry], anchor: Option<Oid>) -> Self {
        let capped = Capped::new(entries.to_vec(), limits::MAX_LOG_PAGE);
        Self {
            commits: capped.items.iter().map(CommitDto::from).collect(),
            truncated: capped.truncated,
            head: anchor.map(|oid| oid.to_string()),
        }
    }
}
