use anyhow::{Context, Result};
use git2::{Oid, Repository};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Upper bound on oids collected per divergence side: the walk yields
/// newest-first, so capping drops the far tail, not the rows a user can
/// actually scroll to.
const MAX_DIVERGENCE_OIDS: usize = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefKind {
    /// The branch HEAD points at, or a detached HEAD.
    Head,
    LocalBranch,
    Tag,
    RemoteBranch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLabel {
    pub kind: RefKind,
    /// Shorthand form (`dev`, `origin/dev`, `v1.2.0`). For a detached HEAD
    /// this is `HEAD`.
    pub name: String,
}

/// Decorations for the commit log: which refs point at which commit, and
/// which commits are ahead of / behind the upstream. Built from refs alone, so
/// callers rebuild it when [`refs_fingerprint`] changes rather than per frame.
#[derive(Debug, Default)]
pub struct LogDecorations {
    labels: HashMap<Oid, Vec<RefLabel>>,
    ahead: HashSet<Oid>,
    behind: HashSet<Oid>,
    head: Option<Oid>,
}

impl LogDecorations {
    pub fn labels_for(&self, oid: Oid) -> &[RefLabel] {
        self.labels.get(&oid).map_or(&[], Vec::as_slice)
    }

    pub fn is_ahead(&self, oid: Oid) -> bool {
        self.ahead.contains(&oid)
    }

    pub fn is_behind(&self, oid: Oid) -> bool {
        self.behind.contains(&oid)
    }

    pub fn is_head(&self, oid: Oid) -> bool {
        self.head == Some(oid)
    }
}

/// Cheap summary of every ref's name and target, to decide whether
/// [`load_log_decorations`] needs to run again. A fetch that advances
/// `origin/dev` changes this even though HEAD did not move.
pub fn refs_fingerprint(repo: &Repository) -> u64 {
    let Ok(refs) = repo.references() else {
        return 0;
    };
    // Order-independent so libgit2's iteration order is not part of the
    // contract: each ref hashes on its own and the digests are summed.
    let mut sum: u64 = 0;
    for reference in refs.flatten() {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        reference.name_bytes().hash(&mut hasher);
        reference
            .target()
            .map(|oid| oid.as_bytes().to_vec())
            .hash(&mut hasher);
        sum = sum.wrapping_add(hasher.finish());
    }
    sum
}

pub fn load_log_decorations(repo: &Repository) -> Result<LogDecorations> {
    let mut labels: HashMap<Oid, Vec<RefLabel>> = HashMap::new();

    for reference in repo
        .references()
        .context("failed to list references")?
        .flatten()
    {
        let kind = if reference.is_branch() {
            RefKind::LocalBranch
        } else if reference.is_remote() {
            RefKind::RemoteBranch
        } else if reference.is_tag() {
            RefKind::Tag
        } else {
            // refs/stash, refs/notes/*, and the symbolic HEAD itself — HEAD is
            // folded into its branch label below instead of listed twice.
            continue;
        };
        // Peels rather than reading `target()` so an annotated tag decorates
        // the commit it points at instead of its own tag object.
        let Ok(commit) = reference.peel_to_commit() else {
            continue;
        };
        let Some(name) = reference.shorthand().ok().map(String::from) else {
            continue;
        };
        labels
            .entry(commit.id())
            .or_default()
            .push(RefLabel { kind, name });
    }

    let head_ref = repo.head().ok();
    let head = head_ref.as_ref().and_then(|h| h.target());
    if let Some(head_oid) = head {
        let branch = head_ref
            .as_ref()
            .filter(|h| h.is_branch())
            .and_then(|h| h.shorthand().ok().map(String::from));
        let entry = labels.entry(head_oid).or_default();
        match branch {
            // Promote the local branch label rather than adding a second one,
            // so the row reads `HEAD -> dev` and not `HEAD  dev`.
            Some(name) => {
                if let Some(label) = entry
                    .iter_mut()
                    .find(|l| l.kind == RefKind::LocalBranch && l.name == name)
                {
                    label.kind = RefKind::Head;
                } else {
                    entry.push(RefLabel {
                        kind: RefKind::Head,
                        name,
                    });
                }
            }
            None => entry.push(RefLabel {
                kind: RefKind::Head,
                name: "HEAD".to_string(),
            }),
        }
    }

    for chips in labels.values_mut() {
        chips.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    }

    let (ahead, behind) = divergence_oids(repo).unwrap_or_default();
    Ok(LogDecorations {
        labels,
        ahead,
        behind,
        head,
    })
}

/// Oids on exactly one side of the HEAD/upstream split. `None` when HEAD is
/// detached, unborn, or has no upstream — nothing to diverge from, not an error.
fn divergence_oids(repo: &Repository) -> Option<(HashSet<Oid>, HashSet<Oid>)> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    let local = head.target()?;
    let upstream = git2::Branch::wrap(head).upstream().ok()?.get().target()?;
    Some((
        exclusive_oids(repo, local, upstream),
        exclusive_oids(repo, upstream, local),
    ))
}

/// Commits reachable from `from` but not from `hidden` — `git rev-list from ^hidden`.
fn exclusive_oids(repo: &Repository, from: Oid, hidden: Oid) -> HashSet<Oid> {
    let Ok(mut revwalk) = repo.revwalk() else {
        return HashSet::new();
    };
    if revwalk.push(from).is_err() || revwalk.hide(hidden).is_err() {
        return HashSet::new();
    }
    revwalk.flatten().take(MAX_DIVERGENCE_OIDS).collect()
}
