use anyhow::{Context, Result};
use git2::{
    Branch, Diff, DiffDelta, DiffOptions, Oid, Repository, Status, StatusEntry, StatusOptions,
};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;

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
    pub time: i64,
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
            time,
        }
    }
}

impl std::fmt::Display for CommitEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.short_id, self.summary)
    }
}

fn load_tracking_status(repo: &Repository) -> Option<TrackingStatus> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    let branch = Branch::wrap(head);
    let upstream = branch.upstream().ok()?;
    let local_oid = branch.get().target()?;
    let upstream_oid = upstream.get().target()?;
    let (ahead, behind) = repo.graph_ahead_behind(local_oid, upstream_oid).ok()?;
    Some(TrackingStatus { ahead, behind })
}

pub fn load_snapshot(repo: &Repository) -> Result<RepoSnapshot> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get repository status")?;

    // Keyed by effective (new-side) path so the file list stays in a stable
    // sorted order across refreshes — selection restoration depends on that.
    // Each git status entry already carries both X and Y bits, so there is no
    // longer a first-wins collapse: one entry maps to one row.
    let mut files = BTreeMap::new();
    for entry in statuses.iter() {
        let Some((index, worktree)) = status_columns(entry.status()) else {
            continue;
        };
        let Some((path, old_path)) = paths_from_status_entry(&entry) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        files.insert(
            path.clone(),
            ChangedFile::from_status_columns(path, old_path, index, worktree),
        );
    }

    let files = files.into_values().collect();

    let tracking = load_tracking_status(repo);
    let head = repo.head().ok();
    let head_oid = head.as_ref().and_then(|h| h.target());
    let branch_name = head
        .as_ref()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().map(String::from));
    Ok(RepoSnapshot {
        files,
        tracking,
        head_oid,
        branch_name,
    })
}

pub const MAX_FILE_VIEW_BYTES: usize = 5 * 1024 * 1024;

/// Parse the new-side starting line from a unified-diff hunk header like
/// `@@ -1,3 +5,7 @@ context`. Returns `None` for synthetic headers
/// (`diff <path>`, `Binary file ...`) or anything malformed.
pub fn parse_hunk_new_start(header: &str) -> Option<usize> {
    let rest = header.strip_prefix("@@ ")?;
    let after = rest.split_once(" +")?.1;
    let token: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if token.is_empty() {
        return None;
    }
    token.parse().ok()
}

fn decode_file_view(bytes: &[u8]) -> Result<String> {
    if bytes.len() > MAX_FILE_VIEW_BYTES {
        return Err(anyhow::anyhow!(
            "file too large to preview: {} bytes",
            bytes.len()
        ));
    }
    std::str::from_utf8(bytes)
        .map(String::from)
        .map_err(|_| anyhow::anyhow!("binary or non-utf8 file"))
}

pub fn load_workdir_file(repo: &Repository, file_path: &str) -> Result<String> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| anyhow::anyhow!("bare repository"))?;
    let full = workdir.join(file_path);
    let meta =
        std::fs::symlink_metadata(&full).with_context(|| format!("failed to stat {file_path}"))?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(&full)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "<unreadable target>".to_string());
        return Err(anyhow::anyhow!(
            "symlink preview disabled: {file_path} -> {target}"
        ));
    }
    // Stat first so a multi-GB log file or build artifact can be rejected
    // without ever materializing into memory: `decode_file_view`'s post-read
    // length check otherwise allocates the full buffer before bailing.
    let len = meta.len();
    if len > MAX_FILE_VIEW_BYTES as u64 {
        return Err(anyhow::anyhow!("file too large to preview: {len} bytes"));
    }
    let bytes = std::fs::read(&full).with_context(|| format!("failed to read {file_path}"))?;
    decode_file_view(&bytes)
}

pub fn load_commit_file_blob(
    repo: &Repository,
    oid: Oid,
    file_path: &str,
    status: StatusKind,
) -> Result<String> {
    let commit = repo.find_commit(oid).context("failed to find commit")?;
    let tree = if status == StatusKind::Deleted {
        commit
            .parent(0)
            .context("deleted file has no parent commit")?
            .tree()
            .context("failed to get parent tree")?
    } else {
        commit.tree().context("failed to get commit tree")?
    };
    let entry = tree
        .get_path(std::path::Path::new(file_path))
        .with_context(|| format!("path not in commit: {file_path}"))?;
    let blob = repo.find_blob(entry.id()).context("failed to read blob")?;
    decode_file_view(blob.content())
}

pub fn load_file_diff(repo: &Repository, file_path: &str) -> Result<Vec<DiffHunk>> {
    let head_tree = repo.head().ok().and_then(|head| head.peel_to_tree().ok());
    let mut diff_opts = diff_options(Some(file_path));

    let mut diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .context("failed to get diff")?;

    diff.find_similar(None)
        .context("failed to detect renamed files")?;

    collect_diff_hunks(&diff, file_path)
}

pub fn load_commit_log(repo: &Repository, max_count: usize) -> Result<Vec<CommitEntry>> {
    load_commit_log_page(repo, 0, max_count)
}

/// Load a slice of the commit log walking back from HEAD.
///
/// `skip` discards the most recent commits before collecting `limit` entries.
/// Callers paginating the log pass the count already loaded as `skip` so the
/// next slice continues from the existing tail.
pub fn load_commit_log_page(
    repo: &Repository,
    skip: usize,
    limit: usize,
) -> Result<Vec<CommitEntry>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    if repo
        .is_empty()
        .context("failed to inspect repository state")?
    {
        return Ok(Vec::new());
    }
    let mut revwalk = repo.revwalk().context("failed to create revwalk")?;
    if let Err(err) = revwalk.push_head() {
        if is_empty_head(&err) {
            return Ok(Vec::new());
        }
        return Err(err).context("failed to push HEAD");
    }

    let mut entries = Vec::with_capacity(limit);
    for oid_result in revwalk.skip(skip).take(limit) {
        let oid = oid_result.context("revwalk error")?;
        let commit = repo.find_commit(oid).context("failed to find commit")?;
        let summary = commit.summary().unwrap_or("").to_string();
        let author = commit.author().name().unwrap_or("Unknown").to_string();
        let time = commit.time().seconds();
        entries.push(CommitEntry::new(oid, short_oid(oid), summary, author, time));
    }
    Ok(entries)
}

/// Render a commit oid as the conventional 7-character abbreviated form.
///
/// Previously this used `repo.find_object(...).short_id()`, which asks
/// libgit2 to compute the *minimum unique prefix length* — at the cost of
/// roughly O(log n) ODB lookups per commit. For a repo with thousands of
/// commits that cost was paid on every initial commit log load. git's own
/// default `core.abbrev` is 7, so a fixed 7-char prefix matches the
/// familiar form while making this an O(1) operation. Oid hex strings are
/// always 40 ASCII bytes, so the slice is sound.
pub(crate) fn short_oid(oid: Oid) -> String {
    let s = oid.to_string();
    s.get(..7).unwrap_or(&s).to_string()
}

fn is_empty_head(err: &git2::Error) -> bool {
    // libgit2 reports "reference 'refs/heads/<branch>' not found" for empty
    // repos with a class of Reference but a generic error code, so we keep
    // the message fallback. libgit2 does not localize internal messages, so
    // the match is portable.
    let missing_head_reference =
        err.class() == git2::ErrorClass::Reference && err.message().contains("not found");

    matches!(
        err.code(),
        git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound
    ) || missing_head_reference
}

fn commit_diff<'repo>(
    repo: &'repo Repository,
    oid: Oid,
    pathspec: Option<&str>,
) -> Result<git2::Diff<'repo>> {
    let commit = repo.find_commit(oid).context("failed to find commit")?;
    let new_tree = commit.tree().context("failed to get commit tree")?;
    // Distinguish a true root commit (no parents) from a parent-lookup
    // failure on a non-root commit — bare `.ok()` previously rendered both
    // merge commits (when parent objects were unreachable) and corrupt
    // history as if the entire tree had just been added.
    let old_tree = if commit.parent_count() == 0 {
        None
    } else {
        Some(
            commit
                .parent(0)
                .context("failed to load parent commit")?
                .tree()
                .context("failed to load parent tree")?,
        )
    };
    let mut diff_opts = diff_options(pathspec);
    let mut diff = repo
        .diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut diff_opts))
        .context("failed to get commit diff")?;
    diff.find_similar(None)
        .context("failed to detect renames")?;
    Ok(diff)
}

pub fn load_commit_files(repo: &Repository, oid: Oid) -> Result<Vec<ChangedFile>> {
    let diff = commit_diff(repo, oid, None)?;
    let mut files = Vec::new();
    for delta in diff.deltas() {
        let kind = match delta.status() {
            git2::Delta::Added => StatusKind::Added,
            git2::Delta::Deleted => StatusKind::Deleted,
            git2::Delta::Renamed => StatusKind::Renamed,
            git2::Delta::Typechange => StatusKind::TypeChanged,
            _ => StatusKind::Modified,
        };
        // New side is the effective path; carry the old side for renames so
        // commit drill-down also renders `old -> new`.
        let path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        let old_path = if kind == StatusKind::Renamed {
            delta
                .old_file()
                .path()
                .map(|p| p.to_string_lossy().to_string())
                .filter(|old| old != &path)
        } else {
            None
        };
        files.push(ChangedFile::from_commit_delta(path, old_path, kind));
    }
    Ok(files)
}

pub fn load_commit_file_diff(
    repo: &Repository,
    oid: Oid,
    file_path: &str,
) -> Result<Vec<DiffHunk>> {
    let diff = commit_diff(repo, oid, Some(file_path))?;
    collect_commit_diff_hunks(&diff)
}

pub fn load_commit_diff(repo: &Repository, oid: Oid) -> Result<Vec<DiffHunk>> {
    let diff = commit_diff(repo, oid, None)?;
    collect_commit_diff_hunks(&diff)
}

fn diff_options(pathspec: Option<&str>) -> DiffOptions {
    let mut opts = DiffOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .show_binary(true);
    if let Some(pathspec) = pathspec {
        opts.pathspec(pathspec).disable_pathspec_match(true);
    }
    opts
}

/// Shared hunk/line accumulation logic. `on_file` returns `Some(hunk)` to prepend a
/// synthetic header entry per file (used by commit diff), or `None` to skip (status diff).
fn collect_hunks(
    diff: &Diff<'_>,
    mut on_file: impl FnMut(DiffDelta<'_>) -> Option<DiffHunk>,
    binary_fallback: &str,
) -> Result<Vec<DiffHunk>> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());
    // Tracks the current file's path between callbacks. libgit2 invokes
    // file_cb once per delta, followed by hunk_cb/line_cb for that file —
    // hunk_cb itself isn't given the delta, so we stash the path here.
    let current_path: RefCell<Option<String>> = RefCell::new(None);

    diff.foreach(
        &mut |delta, _| {
            *current_path.borrow_mut() = path_from_delta(&delta);
            if let Some(h) = on_file(delta) {
                hunks.borrow_mut().push(h);
            }
            true
        },
        Some(&mut |delta, _| {
            let path = path_from_delta(&delta).unwrap_or_else(|| binary_fallback.to_string());
            hunks.borrow_mut().push(binary_diff_hunk(&path));
            true
        }),
        Some(&mut |_, hunk| {
            let header = std::str::from_utf8(hunk.header())
                .unwrap_or("@@")
                .trim_end_matches('\n')
                .to_string();
            hunks.borrow_mut().push(DiffHunk {
                header,
                lines: Vec::new(),
                file_path: current_path.borrow().clone(),
            });
            true
        }),
        Some(&mut |_, _, line| {
            let content = std::str::from_utf8(line.content())
                .unwrap_or("")
                .trim_end_matches('\n')
                .to_string();
            let kind = match line.origin() {
                '+' => LineKind::Added,
                '-' => LineKind::Removed,
                '\\' => return true,
                _ => LineKind::Context,
            };
            if let Some(h) = hunks.borrow_mut().last_mut() {
                h.lines.push(DiffLine { kind, content });
            }
            true
        }),
    )?;

    Ok(hunks.into_inner())
}

fn collect_diff_hunks(diff: &Diff<'_>, fallback_path: &str) -> Result<Vec<DiffHunk>> {
    collect_hunks(diff, |_| None, fallback_path)
}

fn collect_commit_diff_hunks(diff: &Diff<'_>) -> Result<Vec<DiffHunk>> {
    collect_hunks(
        diff,
        |delta| {
            let path = path_from_delta(&delta).unwrap_or_else(|| "unknown".to_string());
            Some(DiffHunk {
                header: format!("diff {path}"),
                lines: Vec::new(),
                file_path: Some(path),
            })
        },
        "unknown",
    )
}

/// Map a git2 status bitset into separate index (X) and worktree (Y) columns.
/// Untracked and conflicted are reported as both-column sentinels so the
/// renderer can collapse them to `??` / `UU`. Returns `None` when neither
/// column carries a displayable change.
fn status_columns(status: Status) -> Option<(StatusKind, StatusKind)> {
    // Untracked: git renders `??` (both columns), not ` ?`. Only a *purely*
    // untracked entry collapses to `??`. A combined state such as
    // `INDEX_DELETED | WT_NEW` (staged deletion, then a fresh file recreated at
    // the same path) keeps its index status so the staged change is not hidden;
    // git itself emits two rows there, but our one-row-per-path model preserves
    // the index side (`D `) rather than masking it as untracked.
    let index_bits = Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE;
    if status.contains(Status::WT_NEW) && !status.intersects(index_bits) {
        return Some((StatusKind::Untracked, StatusKind::Untracked));
    }
    // Conflicts render as `UU` in the first pass; the structured columns keep
    // room for the full unmerged matrix later.
    if status.contains(Status::CONFLICTED) {
        return Some((StatusKind::Unmerged, StatusKind::Unmerged));
    }

    let index = if status.contains(Status::INDEX_NEW) {
        StatusKind::Added
    } else if status.contains(Status::INDEX_MODIFIED) {
        StatusKind::Modified
    } else if status.contains(Status::INDEX_DELETED) {
        StatusKind::Deleted
    } else if status.contains(Status::INDEX_RENAMED) {
        StatusKind::Renamed
    } else if status.contains(Status::INDEX_TYPECHANGE) {
        StatusKind::TypeChanged
    } else {
        StatusKind::Unmodified
    };

    let worktree = if status.contains(Status::WT_MODIFIED) {
        StatusKind::Modified
    } else if status.contains(Status::WT_DELETED) {
        StatusKind::Deleted
    } else if status.contains(Status::WT_RENAMED) {
        StatusKind::Renamed
    } else if status.contains(Status::WT_TYPECHANGE) {
        StatusKind::TypeChanged
    } else if status.contains(Status::WT_UNREADABLE) {
        // No standard git short code; keep it visible as a worktree change
        // rather than dropping the row (preserves prior behavior).
        StatusKind::Modified
    } else {
        StatusKind::Unmodified
    };

    if index == StatusKind::Unmodified && worktree == StatusKind::Unmodified {
        return None;
    }
    Some((index, worktree))
}

/// Effective (new-side) path plus the old path for renames. The effective
/// path drives diff/file loading; `old_path` is display/search metadata only
/// and is omitted when it equals the effective path.
fn paths_from_status_entry(entry: &StatusEntry<'_>) -> Option<(String, Option<String>)> {
    let i2w = entry.index_to_workdir();
    let h2i = entry.head_to_index();
    let status = entry.status();

    let path = i2w
        .as_ref()
        .and_then(new_path_from_delta)
        .or_else(|| h2i.as_ref().and_then(new_path_from_delta))
        .or_else(|| entry.path().map(str::to_string))?;

    let old_path = if status.intersects(Status::INDEX_RENAMED | Status::WT_RENAMED) {
        // Prefer the HEAD-side original when the index carries a rename, so a
        // double-rename (`INDEX_RENAMED | WT_RENAMED`) reports the true original
        // path rather than the intermediate staged name. Fall back to the
        // worktree side for a pure unstaged rename.
        let from = if status.contains(Status::INDEX_RENAMED) {
            h2i.as_ref()
        } else {
            i2w.as_ref()
        };
        from.and_then(old_path_from_delta)
            .filter(|old| old != &path)
    } else {
        None
    };

    Some((path, old_path))
}

fn new_path_from_delta(delta: &DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .map(|p| p.to_string_lossy().to_string())
}

fn old_path_from_delta(delta: &DiffDelta<'_>) -> Option<String> {
    delta
        .old_file()
        .path()
        .map(|p| p.to_string_lossy().to_string())
}

fn path_from_delta(delta: &DiffDelta<'_>) -> Option<String> {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().to_string())
}

fn binary_diff_hunk(file_path: &str) -> DiffHunk {
    DiffHunk {
        header: format!("Binary file {file_path} changed"),
        lines: vec![DiffLine {
            kind: LineKind::Context,
            content: "Binary files differ".to_string(),
        }],
        file_path: Some(file_path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{make_repo, open_repo, run_git};
    use std::path::Path;

    #[test]
    fn snapshot_empty_repo_does_not_panic() {
        let (dir, path) = make_repo();
        let _ = load_snapshot(&open_repo(&path));
        drop(dir);
    }

    #[test]
    fn commit_log_empty_repo_returns_empty() {
        let (dir, path) = make_repo();

        let commits = load_commit_log(&open_repo(&path), 10).unwrap();

        assert!(commits.is_empty());
        drop(dir);
    }

    #[test]
    fn commit_log_page_empty_repo_returns_empty() {
        let (dir, path) = make_repo();

        let page = load_commit_log_page(&open_repo(&path), 0, 5).unwrap();

        assert!(page.is_empty());
        drop(dir);
    }

    #[test]
    fn commit_log_page_zero_limit_returns_empty() {
        let (dir, path) = make_repo();
        std::fs::write(Path::new(&path).join("f"), "x").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "c1"]);

        let page = load_commit_log_page(&open_repo(&path), 0, 0).unwrap();

        assert!(page.is_empty());
        drop(dir);
    }

    #[test]
    fn commit_log_page_paginates_via_skip() {
        let (dir, path) = make_repo();
        for i in 0..5 {
            std::fs::write(Path::new(&path).join(format!("f{i}")), format!("{i}")).unwrap();
            run_git(&path, &["add", "."]);
            run_git(&path, &["commit", "-m", &format!("c{i}")]);
        }

        let first = load_commit_log_page(&open_repo(&path), 0, 2).unwrap();
        let second = load_commit_log_page(&open_repo(&path), 2, 2).unwrap();
        let third = load_commit_log_page(&open_repo(&path), 4, 2).unwrap();

        // Newest first: c4, c3 | c2, c1 | c0.
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].summary, "c4");
        assert_eq!(first[1].summary, "c3");
        assert_eq!(second.len(), 2);
        assert_eq!(second[0].summary, "c2");
        assert_eq!(second[1].summary, "c1");
        assert_eq!(third.len(), 1);
        assert_eq!(third[0].summary, "c0");
        drop(dir);
    }

    #[test]
    fn commit_log_page_skip_beyond_history_returns_empty() {
        let (dir, path) = make_repo();
        std::fs::write(Path::new(&path).join("f"), "x").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "only"]);

        let page = load_commit_log_page(&open_repo(&path), 5, 10).unwrap();

        assert!(page.is_empty());
        drop(dir);
    }

    #[test]
    fn is_empty_head_recognizes_unborn_branch_error() {
        // Drive the actual error path: a freshly-initialized repo has no
        // HEAD target, so revwalk.push_head() returns the error variant our
        // helper must recognize. This guards against libgit2 changing the
        // error class/code combination it reports.
        let (dir, path) = make_repo();
        let repo = open_repo(&path);
        let mut revwalk = repo.revwalk().unwrap();
        let err = revwalk
            .push_head()
            .expect_err("empty repo should fail to push HEAD");
        assert!(
            is_empty_head(&err),
            "is_empty_head failed to recognize unborn HEAD error: \
             class={:?} code={:?} message={}",
            err.class(),
            err.code(),
            err.message()
        );
        drop(dir);
    }

    #[test]
    fn root_commit_diff_lists_added_files() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("first.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);

        let commits = load_commit_log(&open_repo(&path), 1).unwrap();
        let files = load_commit_files(&open_repo(&path), commits[0].oid).unwrap();
        let hunks = load_commit_diff(&open_repo(&path), commits[0].oid).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "first.rs");
        assert_eq!(files[0].index, StatusKind::Added);
        assert_eq!(files[0].worktree, StatusKind::Unmodified);
        assert!(
            hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|line| line.kind == LineKind::Added && line.content.contains("fn main"))
        );
        drop(dir);
    }

    #[test]
    fn snapshot_detects_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("a.txt");
        std::fs::write(&fp, "line1\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "line1\nline2\n").unwrap();

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        assert!(snap.files.iter().any(|f| f.path.contains("a.txt")));
        drop(dir);
    }

    #[test]
    fn snapshot_detects_staged_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("a.txt");
        std::fs::write(&fp, "line1\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "line1\nline2\n").unwrap();
        run_git(&path, &["add", "a.txt"]);

        let snap = load_snapshot(&open_repo(&path)).unwrap();

        assert!(snap.files.iter().any(|f| f.path == "a.txt"
            && f.index == StatusKind::Modified
            && f.worktree == StatusKind::Unmodified
            && f.short_code() == "M "));
        drop(dir);
    }

    #[test]
    fn diff_returns_hunks_for_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("b.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();

        let hunks = load_file_diff(&open_repo(&path), "b.rs").unwrap();
        assert!(!hunks.is_empty());
        assert!(hunks[0].lines.iter().any(|l| l.kind == LineKind::Added));
        drop(dir);
    }

    #[test]
    fn diff_returns_hunks_for_staged_modified_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("b.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        run_git(&path, &["add", "b.rs"]);

        let hunks = load_file_diff(&open_repo(&path), "b.rs").unwrap();

        assert!(!hunks.is_empty());
        assert!(hunks[0].lines.iter().any(|l| l.kind == LineKind::Added));
        drop(dir);
    }

    #[test]
    fn snapshot_detects_staged_added_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("new.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "new.rs"]);

        let snap = load_snapshot(&open_repo(&path)).unwrap();

        assert!(
            snap.files.iter().any(|f| f.path == "new.rs"
                && f.index == StatusKind::Added
                && f.short_code() == "A ")
        );
        drop(dir);
    }

    #[test]
    fn diff_returns_added_lines_for_staged_added_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("new.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "new.rs"]);

        let hunks = load_file_diff(&open_repo(&path), "new.rs").unwrap();

        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines[0].kind, LineKind::Added);
        drop(dir);
    }

    #[test]
    fn diff_returns_added_lines_for_untracked_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("new.rs");
        std::fs::write(&fp, "fn main() {}\n").unwrap();

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        assert!(
            snap.files
                .iter()
                .any(|f| { f.path == "new.rs" && f.short_code() == "??" })
        );

        let hunks = load_file_diff(&open_repo(&path), "new.rs").unwrap();
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines[0].kind, LineKind::Added);
        drop(dir);
    }

    #[test]
    fn snapshot_recurses_untracked_directories() {
        let (dir, path) = make_repo();
        let nested = Path::new(&path).join("src").join("new.rs");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();
        std::fs::write(&nested, "fn main() {}\n").unwrap();

        let snap = load_snapshot(&open_repo(&path)).unwrap();

        assert!(snap.files.iter().any(|f| f.path == "src/new.rs"));
        drop(dir);
    }

    #[test]
    fn diff_returns_placeholder_for_binary_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("asset.bin");
        std::fs::write(&fp, [0, 1, 2]).unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, [0, 1, 3]).unwrap();

        let hunks = load_file_diff(&open_repo(&path), "asset.bin").unwrap();

        assert_eq!(hunks.len(), 1);
        assert!(hunks[0].header.contains("Binary file"));
        drop(dir);
    }

    #[test]
    fn commit_files_detects_renamed_file() {
        let (dir, path) = make_repo();
        let old_path = Path::new(&path).join("old.rs");
        std::fs::write(&old_path, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        run_git(&path, &["mv", "old.rs", "new.rs"]);
        run_git(&path, &["commit", "-m", "rename"]);

        let commits = load_commit_log(&open_repo(&path), 1).unwrap();
        let files = load_commit_files(&open_repo(&path), commits[0].oid).unwrap();

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "new.rs");
        assert_eq!(files[0].index, StatusKind::Renamed);
        assert_eq!(files[0].old_path.as_deref(), Some("old.rs"));
        assert_eq!(files[0].display_path(), "old.rs -> new.rs");
        drop(dir);
    }

    #[test]
    fn parse_hunk_new_start_handles_standard_header() {
        assert_eq!(parse_hunk_new_start("@@ -1,3 +5,7 @@"), Some(5));
        assert_eq!(parse_hunk_new_start("@@ -10 +12 @@ ctx"), Some(12));
        assert_eq!(parse_hunk_new_start("@@ -0,0 +1,4 @@"), Some(1));
        assert_eq!(parse_hunk_new_start("diff src/foo.rs"), None);
        assert_eq!(parse_hunk_new_start("Binary file x changed"), None);
        assert_eq!(parse_hunk_new_start("@@"), None);
    }

    #[test]
    fn load_workdir_file_reads_text_file() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("hello.txt");
        std::fs::write(&fp, "hi\nthere\n").unwrap();
        let content = load_workdir_file(&open_repo(&path), "hello.txt").unwrap();
        assert_eq!(content, "hi\nthere\n");
        drop(dir);
    }

    #[test]
    fn load_workdir_file_rejects_binary() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("bin");
        std::fs::write(&fp, [0x00, 0xff, 0xfe]).unwrap();
        assert!(load_workdir_file(&open_repo(&path), "bin").is_err());
        drop(dir);
    }

    #[cfg(unix)]
    #[test]
    fn load_workdir_file_rejects_symlink_without_following() {
        let (dir, path) = make_repo();
        let target = Path::new(&path).join("target.txt");
        std::fs::write(&target, "secret\n").unwrap();
        std::os::unix::fs::symlink(&target, Path::new(&path).join("link.txt")).unwrap();

        let err = load_workdir_file(&open_repo(&path), "link.txt").unwrap_err();

        assert!(err.to_string().contains("symlink preview disabled"));
        drop(dir);
    }

    #[test]
    fn load_commit_file_blob_reads_committed_text() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("a.txt");
        std::fs::write(&fp, "v1\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        std::fs::write(&fp, "v2\n").unwrap();
        let commits = load_commit_log(&open_repo(&path), 1).unwrap();
        let content = load_commit_file_blob(
            &open_repo(&path),
            commits[0].oid,
            "a.txt",
            StatusKind::Modified,
        )
        .unwrap();
        assert_eq!(content, "v1\n");
        drop(dir);
    }

    #[test]
    fn load_commit_file_blob_reads_deleted_file_from_parent() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("gone.txt");
        std::fs::write(&fp, "before delete\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "add file"]);
        std::fs::remove_file(&fp).unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "delete file"]);

        let commits = load_commit_log(&open_repo(&path), 1).unwrap();
        let content = load_commit_file_blob(
            &open_repo(&path),
            commits[0].oid,
            "gone.txt",
            StatusKind::Deleted,
        )
        .unwrap();

        assert_eq!(content, "before delete\n");
        drop(dir);
    }

    #[test]
    fn commit_file_diff_returns_renamed_file_diff() {
        let (dir, path) = make_repo();
        let old_path = Path::new(&path).join("old.rs");
        std::fs::write(&old_path, "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        run_git(&path, &["mv", "old.rs", "new.rs"]);
        std::fs::write(
            Path::new(&path).join("new.rs"),
            "fn main() {\n    println!(\"hi\");\n}\n",
        )
        .unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "rename and edit"]);

        let commits = load_commit_log(&open_repo(&path), 1).unwrap();
        let hunks = load_commit_file_diff(&open_repo(&path), commits[0].oid, "new.rs").unwrap();

        assert!(!hunks.is_empty());
        assert!(
            hunks
                .iter()
                .flat_map(|h| &h.lines)
                .any(|l| l.kind == LineKind::Added && l.content.contains("println"))
        );
        drop(dir);
    }

    // --- Workstream 1: XY status model unit tests (no git needed) ---

    #[test]
    fn short_code_renders_each_xy_combination() {
        let mk = |index, worktree| {
            ChangedFile::from_status_columns("p".into(), None, index, worktree).short_code()
        };
        assert_eq!(mk(StatusKind::Unmodified, StatusKind::Modified), " M");
        assert_eq!(mk(StatusKind::Modified, StatusKind::Unmodified), "M ");
        assert_eq!(mk(StatusKind::Modified, StatusKind::Modified), "MM");
        assert_eq!(mk(StatusKind::Added, StatusKind::Unmodified), "A ");
        assert_eq!(mk(StatusKind::Renamed, StatusKind::Unmodified), "R ");
        assert_eq!(mk(StatusKind::TypeChanged, StatusKind::Unmodified), "T ");
        // Untracked and conflicted collapse to git's two-column sentinels
        // regardless of which column carries the bit.
        assert_eq!(mk(StatusKind::Untracked, StatusKind::Untracked), "??");
        assert_eq!(mk(StatusKind::Unmerged, StatusKind::Unmerged), "UU");
    }

    #[test]
    fn display_path_borrows_for_non_rename_and_formats_rename() {
        let plain = ChangedFile::from_status_columns(
            "src/a.rs".into(),
            None,
            StatusKind::Modified,
            StatusKind::Unmodified,
        );
        assert!(matches!(plain.display_path(), Cow::Borrowed(_)));
        assert_eq!(plain.display_path(), "src/a.rs");

        let renamed = ChangedFile::from_status_columns(
            "new.rs".into(),
            Some("old.rs".into()),
            StatusKind::Renamed,
            StatusKind::Unmodified,
        );
        assert!(matches!(renamed.display_path(), Cow::Owned(_)));
        assert_eq!(renamed.display_path(), "old.rs -> new.rs");
        // Search text matches either side of a rename.
        assert!(renamed.search_lower.contains("old.rs"));
        assert!(renamed.search_lower.contains("new.rs"));
    }

    #[test]
    fn most_severe_picks_higher_severity_column() {
        // Deleted outranks modified regardless of column.
        let f = ChangedFile::from_status_columns(
            "p".into(),
            None,
            StatusKind::Modified,
            StatusKind::Deleted,
        );
        assert_eq!(f.most_severe(), StatusKind::Deleted);
        let f = ChangedFile::from_status_columns(
            "p".into(),
            None,
            StatusKind::Deleted,
            StatusKind::Modified,
        );
        assert_eq!(f.most_severe(), StatusKind::Deleted);
    }

    // --- Workstream 1: git status -> XY mapping tests ---

    fn find<'a>(snap: &'a RepoSnapshot, path: &str) -> &'a ChangedFile {
        snap.files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path} missing from snapshot"))
    }

    #[test]
    fn snapshot_distinguishes_staged_and_unstaged_modification() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("a.txt");
        std::fs::write(&fp, "v1\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        // Stage one modification, then modify again without staging.
        std::fs::write(&fp, "v2\n").unwrap();
        run_git(&path, &["add", "a.txt"]);
        std::fs::write(&fp, "v3\n").unwrap();

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        assert_eq!(find(&snap, "a.txt").short_code(), "MM");
        drop(dir);
    }

    #[test]
    fn snapshot_distinguishes_staged_and_unstaged_deletion() {
        let (dir, path) = make_repo();
        std::fs::write(Path::new(&path).join("staged.txt"), "x\n").unwrap();
        std::fs::write(Path::new(&path).join("wt.txt"), "y\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        // staged deletion (index) vs working-tree deletion (unstaged).
        run_git(&path, &["rm", "staged.txt"]);
        std::fs::remove_file(Path::new(&path).join("wt.txt")).unwrap();

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        assert_eq!(find(&snap, "staged.txt").short_code(), "D ");
        assert_eq!(find(&snap, "wt.txt").short_code(), " D");
        drop(dir);
    }

    #[test]
    fn snapshot_keeps_staged_deletion_visible_when_path_recreated() {
        // `INDEX_DELETED | WT_NEW`: a staged deletion with a fresh untracked
        // file recreated at the same path. git emits two rows (`D ` and `??`);
        // our one-row model must keep the staged deletion rather than masking
        // the whole row as untracked.
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("f.txt");
        std::fs::write(&fp, "orig\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        run_git(&path, &["rm", "--cached", "f.txt"]);
        std::fs::write(&fp, "new content\n").unwrap();

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        let f = find(&snap, "f.txt");
        assert_eq!(f.index, StatusKind::Deleted);
        assert_eq!(f.short_code(), "D ");
        drop(dir);
    }

    #[cfg(unix)]
    #[test]
    fn snapshot_detects_staged_typechange() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("f");
        std::fs::write(&fp, "regular\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        // Replace the regular file with a symlink and stage it.
        std::fs::remove_file(&fp).unwrap();
        std::os::unix::fs::symlink("target", &fp).unwrap();
        run_git(&path, &["add", "f"]);

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        assert_eq!(find(&snap, "f").index, StatusKind::TypeChanged);
        assert_eq!(find(&snap, "f").short_code(), "T ");
        drop(dir);
    }

    #[test]
    fn snapshot_renders_conflicted_file_as_uu() {
        let (dir, path) = make_repo();
        let fp = Path::new(&path).join("c.txt");
        std::fs::write(&fp, "base\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        run_git(&path, &["checkout", "-b", "feature"]);
        std::fs::write(&fp, "feature\n").unwrap();
        run_git(&path, &["commit", "-am", "feature edit"]);
        run_git(&path, &["checkout", "-"]);
        std::fs::write(&fp, "mainline\n").unwrap();
        run_git(&path, &["commit", "-am", "mainline edit"]);
        // Conflicting merge exits non-zero; run it tolerantly.
        let merge = std::process::Command::new("git")
            .args(["merge", "feature"])
            .current_dir(&path)
            .output()
            .unwrap();
        assert!(!merge.status.success(), "merge should conflict");

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        assert_eq!(find(&snap, "c.txt").short_code(), "UU");
        drop(dir);
    }

    #[test]
    fn snapshot_preserves_rename_and_loads_new_side_diff() {
        let (dir, path) = make_repo();
        // Keep content identical across the rename so git's similarity
        // detection reports a staged rename rather than add+delete.
        std::fs::write(Path::new(&path).join("old.rs"), "fn main() {}\n").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", "init"]);
        run_git(&path, &["mv", "old.rs", "new.rs"]);

        let snap = load_snapshot(&open_repo(&path)).unwrap();
        let f = find(&snap, "new.rs");
        assert_eq!(f.index, StatusKind::Renamed);
        assert_eq!(f.old_path.as_deref(), Some("old.rs"));
        assert_eq!(f.display_path(), "old.rs -> new.rs");

        // The effective path is the new side; selecting it must still load a
        // diff without error (regression guard for the rename display change).
        assert!(load_file_diff(&open_repo(&path), &f.path).is_ok());
        drop(dir);
    }
}
