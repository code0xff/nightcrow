use git2::Oid;
use std::borrow::Cow;

/// State of a single git status column. `index` (X) compares HEAD with the
/// staged tree, `worktree` (Y) compares the staged tree with the working
/// directory. Either column can be `Unmodified` — that is what the old
/// single-status `ChangeStatus` could not express. Mirrors the codes used by
/// `git status --short`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Unmodified,
    Added,
    Modified,
    Deleted,
    Renamed,
    TypeChanged,
    Untracked,
    Unmerged,
}

impl StatusKind {
    /// The Git short status character for this column. `Unmodified` is a space
    /// so a single-sided change renders as ` M` / `M `.
    fn code_char(self) -> char {
        match self {
            Self::Unmodified => ' ',
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::TypeChanged => 'T',
            Self::Untracked => '?',
            Self::Unmerged => 'U',
        }
    }

    /// Severity rank used to pick a single color for the two-character code.
    /// Higher wins: unmerged > deleted > renamed > added > modified >
    /// typechanged > untracked > unmodified (see plan Resolved Decisions #3).
    fn severity(self) -> u8 {
        match self {
            Self::Unmerged => 7,
            Self::Deleted => 6,
            Self::Renamed => 5,
            Self::Added => 4,
            Self::Modified => 3,
            Self::TypeChanged => 2,
            Self::Untracked => 1,
            Self::Unmodified => 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChangedFile {
    /// New/effective path. Used for diff loading, file preview, hot-file
    /// tracking, and selection restoration.
    pub path: String,
    /// Old path for renames (display/search metadata only). `None` otherwise.
    pub old_path: Option<String>,
    /// Index column (X): HEAD vs staged.
    pub index: StatusKind,
    /// Working-tree column (Y): staged vs working directory.
    pub worktree: StatusKind,
    /// Pre-computed lowercase search text. For renames it contains both old and
    /// new paths so either side matches the `contains` filter; otherwise it is
    /// the lowercased `path`. Set on construction so the file-list filter
    /// doesn't lowercase on every keystroke.
    pub search_lower: String,
}

impl ChangedFile {
    /// Build from explicit status columns (status snapshot path).
    pub fn from_status_columns(
        path: String,
        old_path: Option<String>,
        index: StatusKind,
        worktree: StatusKind,
    ) -> Self {
        let search_lower = match &old_path {
            Some(old) => format!("{old} {path}").to_lowercase(),
            None => path.to_lowercase(),
        };
        Self {
            path,
            old_path,
            index,
            worktree,
            search_lower,
        }
    }

    /// Build from a commit delta: the single delta status lives in the index
    /// column and the worktree column is `Unmodified`, so commit drill-down
    /// rows render `M `, `A `, `D `, `R `.
    pub fn from_commit_delta(path: String, old_path: Option<String>, kind: StatusKind) -> Self {
        Self::from_status_columns(path, old_path, kind, StatusKind::Unmodified)
    }

    /// Two-character Git short status code (`XY`). Untracked is special-cased
    /// to `??` and conflicts to `UU` to match git rather than emitting ` ?`
    /// from a blank index plus untracked worktree.
    pub fn short_code(&self) -> String {
        if self.index == StatusKind::Untracked || self.worktree == StatusKind::Untracked {
            return "??".to_string();
        }
        if self.index == StatusKind::Unmerged || self.worktree == StatusKind::Unmerged {
            return "UU".to_string();
        }
        let mut code = String::with_capacity(2);
        code.push(self.index.code_char());
        code.push(self.worktree.code_char());
        code
    }

    /// The more severe of the two columns, used to pick the row color.
    pub fn most_severe(&self) -> StatusKind {
        if self.index.severity() >= self.worktree.severity() {
            self.index
        } else {
            self.worktree
        }
    }

    /// Rendered display path. Non-rename borrows `path` with no allocation
    /// (the hot per-frame case); renames own the formatted `old -> new` string.
    /// Returns `Cow<str>` so callers can slice it for horizontal scroll via
    /// `char_offset` and measure it with `chars().count()`.
    pub fn display_path(&self) -> Cow<'_, str> {
        match &self.old_path {
            Some(old) => Cow::Owned(format!("{old} -> {}", self.path)),
            None => Cow::Borrowed(&self.path),
        }
    }

    /// Test-only convenience: an unstaged change of `kind` at `path`
    /// (` X` column blank). Production code uses the explicit constructors.
    #[cfg(test)]
    pub(crate) fn unstaged_only(path: String, kind: StatusKind) -> Self {
        Self::from_status_columns(path, None, StatusKind::Unmodified, kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub content: String,
    /// Line number on the pre-image side, as reported by libgit2. `None` for an
    /// added line, which exists only on the new side — so the gutter can leave
    /// that column blank instead of inventing a number. Also `None` on
    /// hand-built fixtures and the synthetic binary-file hunk, where no real
    /// line numbering exists.
    pub old_lineno: Option<u32>,
    /// Line number on the post-image side. `None` for a removed line, which is
    /// absent from the new side. Same `None` cases as `old_lineno` otherwise.
    pub new_lineno: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
    /// File this hunk belongs to. `Some` for hunks emitted by the diff
    /// collectors below; `None` for hand-built fixtures in tests where the
    /// path is irrelevant. Used by the renderer to pick a per-hunk syntax
    /// in commit diffs (one commit can touch multiple file types).
    pub file_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TrackingStatus {
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone)]
pub struct RepoSnapshot {
    pub files: Vec<ChangedFile>,
    pub tracking: Option<TrackingStatus>,
    /// HEAD commit oid at the moment the snapshot was taken. `None` for
    /// empty or detached repositories with no resolvable HEAD. The main
    /// thread compares this against `App::last_head_oid` to detect new
    /// commits and refresh the Log view's cached commit list.
    pub head_oid: Option<Oid>,
    /// Current branch shorthand (e.g. `main`) when HEAD points at a branch.
    /// `None` for detached HEAD, unborn branch, or bare repo so the header
    /// can decide whether to render the branch chip.
    pub branch_name: Option<String>,
    /// Digest over every ref name and target. Refs move without HEAD moving
    /// (a fetch advances `origin/dev`), so the Log view rebuilds its ref
    /// decoration map when this changes rather than on HEAD changes alone.
    pub refs_fingerprint: u64,
}

#[derive(Debug, Clone)]
pub struct CommitEntry {
    pub oid: Oid,
    pub short_id: String,
    pub summary: String,
    /// Pre-computed lowercase form of `summary` for case-insensitive search.
    /// Set on construction so the commit-log filter doesn't lowercase on every
    /// keystroke. Mirrors `ChangedFile::search_lower`.
    pub summary_lower: String,
    pub author: String,
    /// Author email, shown only in the wide layout. Empty when the commit
    /// carries none.
    pub author_email: String,
    pub time: i64,
    /// Number of parents. `> 1` marks a merge commit; 0 marks a root commit.
    pub parent_count: usize,
}

impl CommitEntry {
    pub fn new(oid: Oid, short_id: String, summary: String, author: String, time: i64) -> Self {
        let summary_lower = summary.to_lowercase();
        Self {
            oid,
            short_id,
            summary,
            summary_lower,
            author,
            author_email: String::new(),
            time,
            parent_count: 1,
        }
    }

    /// Attach the fields that come off the same `git2::Commit` object as the
    /// rest of the entry, so the loader pays no extra ODB lookup for them.
    pub fn with_commit_meta(mut self, author_email: String, parent_count: usize) -> Self {
        self.author_email = author_email;
        self.parent_count = parent_count;
        self
    }

    pub fn is_merge(&self) -> bool {
        self.parent_count > 1
    }
}

impl std::fmt::Display for CommitEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.short_id, self.summary)
    }
}
