//! Recursive filesystem watcher for the whole working tree, so the status reader
//! can react to changes instead of polling. Failure to install is not fatal: the
//! reader falls back to a fixed interval. One inotify descriptor per directory on
//! Linux means a large checkout can be refused outright.

use notify::event::{AccessKind, AccessMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

pub(super) enum Wake {
    /// Filesystem paths that changed. An empty list means something changed that
    /// the watcher could not name — including its own error — which is a reason
    /// to look rather than a reason to stop.
    Changed(Vec<PathBuf>),
    Stop,
}

/// Start watching `root` recursively, or `None` when the platform refuses.
/// The watcher must be kept alive by the caller; dropping it stops the watch.
///
/// A caller that gets `None` must not retry on a timer: the refusals this hits
/// (inotify's watch limit, a directory it may not read) are properties of the
/// machine, and a second attempt a second later re-walks the tree to fail the
/// same way and log the same line.
pub(super) fn install(root: &Path, wake: Sender<Wake>) -> Option<RecommendedWatcher> {
    let mut watcher = match notify::recommended_watcher(move |event: notify::Result<Event>| {
        let Some(paths) = changed_paths(event) else {
            return;
        };
        // The worker is gone once this fails, which is not this thread's
        // business to report.
        let _ = wake.send(Wake::Changed(paths));
    }) {
        Ok(watcher) => watcher,
        Err(err) => {
            tracing::warn!(%err, "no filesystem watcher available; reading on a timer");
            return None;
        }
    };
    match watcher.watch(root, RecursiveMode::Recursive) {
        Ok(()) => Some(watcher),
        Err(err) => {
            tracing::warn!(
                %err,
                root = %root.display(),
                "cannot watch this directory; reading on a timer"
            );
            None
        }
    }
}

/// The paths worth waking the reader for, or `None` when the event cannot mean
/// the tree changed.
///
/// **Reading is an event too, on Linux.** inotify reports `IN_OPEN`, and every
/// status read opens files inside the watched roots (libgit2 reads `HEAD`, the
/// branch ref, and the untracked walk opens the work-tree directory). Without
/// this filter the reader became its own event source — each read scheduled the
/// next one a second later, forever. FSEvents and `ReadDirectoryChangesW` do not
/// report file opens, so this went unnoticed in development.
fn changed_paths(event: notify::Result<Event>) -> Option<Vec<PathBuf>> {
    let event = match event {
        Ok(event) => event,
        Err(err) => {
            tracing::debug!(%err, "filesystem watcher error; reading anyway");
            // It may have missed events, and naming none of them is how that is
            // said — see [`Wake::Changed`].
            return Some(Vec::new());
        }
    };
    match event.kind {
        // The one access that is not somebody looking: the file was open for
        // writing and is now finished, which is `IN_CLOSE_WRITE` and a change.
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => Some(event.paths),
        EventKind::Access(_) => None,
        // Everything else as before, [`EventKind::Other`] included: an inotify
        // queue overflow arrives as `Other` naming no paths, and that empty list
        // is what forces the re-read a dropped event calls for.
        _ => Some(event.paths),
    }
}

/// A directory as given, and as the filesystem reports it.
///
/// Both, because macOS resolves symlinks in the paths it hands back: a repository
/// under `/var/folders/...` is reported under `/private/var/folders/...`. Windows
/// does the same to 8.3 short names. A path that cannot be made relative to the
/// tree is treated as "cannot tell, read it", so getting this wrong silently turns
/// the whole filter off — the same as not having written it.
struct Prefix {
    given: PathBuf,
    canonical: PathBuf,
}

impl Prefix {
    fn of(path: &Path) -> Self {
        Self {
            given: path.to_path_buf(),
            // Cleaned, not raw: `canonicalize` returns the verbatim (`\\?\`)
            // form on Windows, and neither libgit2 nor the watcher ever produces
            // one — so the raw form is a prefix of nothing and this second
            // spelling silently stops being a second spelling.
            canonical: crate::platform::paths::canonicalize_clean(path)
                .unwrap_or_else(|_| path.to_path_buf()),
        }
    }

    fn relative<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        path.strip_prefix(&self.canonical)
            .or_else(|_| path.strip_prefix(&self.given))
            .ok()
    }
}

/// The directories an event can arrive from: the work tree, and the git
/// directory when the repository keeps it somewhere else.
pub(super) struct Roots {
    tree: Prefix,
    /// Set only when the git directory is outside the work tree — `git worktree
    /// add` and `--separate-git-dir` both do that, and then the index and the
    /// refs are nowhere the tree's watch can see them.
    git_dir: Option<Prefix>,
}

impl Roots {
    pub(super) fn of(root: &Path) -> Self {
        Self {
            tree: Prefix::of(root),
            git_dir: None,
        }
    }

    /// `None` clears it, for a repository that turned out to keep its git
    /// directory inside the tree after all.
    pub(super) fn set_external_git_dir(&mut self, git_dir: Option<&Path>) {
        self.git_dir = git_dir.map(Prefix::of);
    }
}

/// The repository's common directory when it does not live inside the watched
/// tree. The *common* directory rather than `path()`: a linked worktree's `path()`
/// holds its index but its refs are in the main repository's, and a commit made
/// elsewhere on the same branch changes what a status says. The common directory
/// contains both.
pub(super) fn external_git_dir(repo: &git2::Repository, roots: &Roots) -> Option<PathBuf> {
    let common = repo.commondir();
    match roots.tree.relative(common) {
        Some(_) => None,
        None => Some(common.to_path_buf()),
    }
}

/// Whether any of `paths` could change what a status says.
///
/// `repo` is used to ask git what it ignores; without a handle open every path
/// counts, since guessing wrong the other way would leave a real change unread.
pub(super) fn any_matters(
    repo: Option<&git2::Repository>,
    roots: &Roots,
    paths: &[PathBuf],
) -> bool {
    // Nothing named: the watcher lost track, so look.
    if paths.is_empty() {
        return true;
    }
    paths.iter().any(|path| matters(repo, roots, path))
}

fn matters(repo: Option<&git2::Repository>, roots: &Roots, path: &Path) -> bool {
    if let Some(git_dir) = &roots.git_dir
        && let Some(inside) = git_dir.relative(path)
    {
        return git_metadata_matters(inside);
    }
    let Some(relative) = roots.tree.relative(path) else {
        // Outside anything watched, which should not be reported. A path that
        // cannot be placed is read rather than dropped.
        return true;
    };
    if let Ok(inside) = relative.strip_prefix(".git") {
        return git_metadata_matters(inside);
    }
    let Some(repo) = repo else {
        return true;
    };
    // Build output is the loudest thing in a working tree and the one thing git
    // has been told to disregard: a `cargo build` writes thousands of files that
    // cannot appear in a status. Skipping them is what makes this worth having.
    //
    // A tracked file inside an ignored directory (added with `-f`) is the case
    // this skips wrongly. The idle read is what still catches it.
    !repo.is_path_ignored(relative).unwrap_or(false)
}

/// Whether a change at `inside` — a path relative to a git directory — could
/// change what a status says.
///
/// **Top level only, on purpose.** A submodule keeps a git directory of its own
/// under `modules/<name>/`, and the same churn happens there, so extending the
/// rule to those is tempting. It cannot be done from the path: a submodule's
/// name is its path in the tree, slashes and all, so `modules/foo/objects/HEAD`
/// is the `HEAD` of a submodule at `foo/objects` and the objects directory of
/// one at `foo` — and there is no counting of components that tells them apart.
/// Guessing costs a real change dropped in one direction and nothing gained in
/// the other, while admitting them all costs at most one extra read per second
/// during a submodule fetch, which is what the reader cost before it watched
/// anything.
fn git_metadata_matters(inside: &Path) -> bool {
    // Objects and reflogs churn on every commit and every fetch, and neither
    // changes a status by itself — the index or ref update that comes with them
    // does, and that is watched.
    if inside.starts_with("objects") || inside.starts_with("logs") {
        return false;
    }
    // Git takes `index.lock` before an operation and removes it after. Reading
    // on that means reading a tree mid-change, and the real event follows
    // immediately.
    inside.extension().is_none_or(|ext| ext != "lock")
}

#[cfg(test)]
#[path = "snapshot_watch_tests.rs"]
mod tests;
