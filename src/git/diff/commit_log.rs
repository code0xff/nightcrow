use crate::git::diff::types::CommitEntry;
use anyhow::{Context, Result};
use git2::{Oid, Repository};

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
    load_commit_log_from(repo, None, skip, limit)
}

/// Load a slice of the commit log walking back from `anchor`, or from HEAD when
/// it is `None`.
///
/// The anchor exists for callers that page across separate requests. `skip` is
/// an offset into one walk, so it only identifies the same commits as long as
/// the walk starts where it did before — and HEAD moves whenever a commit lands
/// while the pages are being collected, which silently shifts every subsequent
/// offset and duplicates or drops entries. Pinning the start makes a sequence of
/// pages describe one history rather than a moving one.
pub fn load_commit_log_from(
    repo: &Repository,
    anchor: Option<Oid>,
    skip: usize,
    limit: usize,
) -> Result<Vec<CommitEntry>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let mut revwalk = repo.revwalk().context("failed to create revwalk")?;
    match anchor {
        // Pushed without consulting `is_empty` first: an anchor names a commit
        // the caller believes exists, so an unknown one is that caller's error
        // to hear about, not a reason to answer with an empty history. (An
        // empty repository has no commit to name, so this always fails there.)
        Some(oid) => revwalk
            .push(oid)
            .with_context(|| format!("failed to push commit {oid}"))?,
        None => {
            if repo
                .is_empty()
                .context("failed to inspect repository state")?
            {
                return Ok(Vec::new());
            }
            if let Err(err) = revwalk.push_head() {
                if is_empty_head(&err) {
                    return Ok(Vec::new());
                }
                return Err(err).context("failed to push HEAD");
            }
        }
    }

    let mut entries = Vec::with_capacity(limit);
    for oid_result in revwalk.skip(skip).take(limit) {
        let oid = oid_result.context("revwalk error")?;
        let commit = repo.find_commit(oid).context("failed to find commit")?;
        let summary = commit.summary().ok().flatten().unwrap_or("").to_string();
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

/// The commit HEAD points at, or `None` when the branch has no commits yet.
///
/// An unborn HEAD is a state, not a failure — a repository is allowed to have no
/// history. Every *other* failure to read the ref is returned, so a broken or
/// unreadable HEAD is reported rather than being flattened into "no commits",
/// which reads to a caller as an empty history it can trust.
pub fn head_commit_oid(repo: &Repository) -> Result<Option<Oid>> {
    match repo.head() {
        Ok(head) => Ok(head.target()),
        // Only the unborn branch, deliberately narrower than [`is_empty_head`].
        // That one also accepts a bare `NotFound` because `revwalk.push_head()`
        // reports an empty repository that way, but `repo.head()` names the
        // state exactly — so here a `NotFound` means the ref is missing or
        // unreadable, which is a broken repository, not an empty one.
        Err(err) if err.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(err) => Err(err).context("failed to read HEAD"),
    }
}

pub(crate) fn is_empty_head(err: &git2::Error) -> bool {
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