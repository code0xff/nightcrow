//! What git is in the middle of: a merge, rebase, cherry-pick, revert or bisect
//! that stopped and is waiting for someone. Without this the status list looks
//! the same as on any other afternoon, and a rebase an agent left half done in
//! a pane is easy to miss.

use git2::{Repository, RepositoryState};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationKind {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    Bisect,
    /// `git am`, stopped on a patch. Distinct from a rebase because getting
    /// out of it is `git am --continue` / `--abort`, not the rebase commands.
    ApplyMailbox,
}

impl OperationKind {
    /// How it is named on screen: upper case, the way `git status` names a
    /// state rather than an action.
    pub fn label(self) -> &'static str {
        match self {
            Self::Merge => "MERGING",
            Self::Rebase => "REBASING",
            Self::CherryPick => "CHERRY-PICKING",
            Self::Revert => "REVERTING",
            Self::Bisect => "BISECTING",
            Self::ApplyMailbox => "APPLYING",
        }
    }

    /// The wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
            Self::Bisect => "bisect",
            Self::ApplyMailbox => "am",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operation {
    pub kind: OperationKind,
    /// Rebase progress, `(step, total)`, when git recorded it.
    pub progress: Option<(u32, u32)>,
}

impl Operation {
    /// `REBASING 2/5`, or the bare label when there is no progress to show.
    pub fn text(&self) -> String {
        match self.progress {
            Some((step, total)) => format!("{} {step}/{total}", self.kind.label()),
            None => self.kind.label().to_string(),
        }
    }
}

/// The operation in progress, or `None` for a repository at rest.
pub fn current_operation(repo: &Repository) -> Option<Operation> {
    let kind = match repo.state() {
        RepositoryState::Clean => return None,
        RepositoryState::Merge => OperationKind::Merge,
        RepositoryState::Revert | RepositoryState::RevertSequence => OperationKind::Revert,
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
            OperationKind::CherryPick
        }
        RepositoryState::Bisect => OperationKind::Bisect,
        RepositoryState::Rebase
        | RepositoryState::RebaseInteractive
        | RepositoryState::RebaseMerge => OperationKind::Rebase,
        RepositoryState::ApplyMailbox => OperationKind::ApplyMailbox,
        // libgit2 cannot tell from `rebase-apply/` alone; `git am` marks its
        // own use of the directory with an `applying` file.
        RepositoryState::ApplyMailboxOrRebase => {
            if repo.path().join("rebase-apply").join("applying").exists() {
                OperationKind::ApplyMailbox
            } else {
                OperationKind::Rebase
            }
        }
    };
    let progress = match kind {
        OperationKind::Rebase | OperationKind::ApplyMailbox => rebase_progress(repo.path()),
        _ => None,
    };
    Some(Operation { kind, progress })
}

/// Read the step counters git writes for a rebase in progress. The merge
/// backend keeps `msgnum`/`end` under `rebase-merge`; the apply backend keeps
/// `next`/`last` under `rebase-apply`. Absent or unreadable is not an error —
/// the state is still shown, just without a count.
fn rebase_progress(git_dir: &Path) -> Option<(u32, u32)> {
    let read = |dir: &str, name: &str| -> Option<u32> {
        std::fs::read_to_string(git_dir.join(dir).join(name))
            .ok()?
            .trim()
            .parse()
            .ok()
    };
    let pair = |dir: &str, step: &str, total: &str| -> Option<(u32, u32)> {
        let (step, total) = (read(dir, step)?, read(dir, total)?);
        (total > 0 && step <= total).then_some((step, total))
    };
    pair("rebase-merge", "msgnum", "end").or_else(|| pair("rebase-apply", "next", "last"))
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
