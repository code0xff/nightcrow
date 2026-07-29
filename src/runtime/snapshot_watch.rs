//! Noticing that a working tree changed, so the status reader can stop guessing.
//!
//! A `git status` walk lstats every tracked file — 3 ms on a small repository,
//! 129 ms on one with fifty thousand files (measured) — and running it on a timer
//! spends that whether or not anything happened. This watches instead, so an idle
//! repository costs nothing and a changed one is read at once.
//!
//! **Recursive, unlike the file-tree watcher next door.** That one watches only
//! the directories a user expanded, because a listing is per-directory; a status
//! covers the whole tree, so there is no smaller set that would answer the
//! question. The cost of that is real on Linux — one inotify descriptor per
//! directory, and a large checkout can be refused outright — so a failure to
//! install is not fatal: the reader falls back to the fixed interval it used
//! before this existed.

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

/// What the snapshot worker waits for.
pub(super) enum Wake {
    /// The filesystem reported these paths. An empty list means something
    /// changed that the watcher could not name — including its own error — which
    /// is a reason to look rather than a reason to stop.
    Changed(Vec<PathBuf>),
    /// The channel is going away.
    Stop,
}

/// Start watching `root` and everything under it, or `None` when the platform
/// refuses. The watcher must be kept alive by the caller; dropping it stops the
/// watch.
///
/// A caller that gets `None` must not retry on a timer: the refusals this hits
/// (inotify's watch limit, a directory it may not read) are properties of the
/// machine, and a second attempt a second later re-walks the tree to fail the
/// same way and log the same line.
pub(super) fn install(root: &Path, wake: Sender<Wake>) -> Option<RecommendedWatcher> {
    let mut watcher = match notify::recommended_watcher(move |event: notify::Result<Event>| {
        let paths = match event {
            Ok(event) => event.paths,
            Err(err) => {
                tracing::debug!(%err, "filesystem watcher error; reading anyway");
                Vec::new()
            }
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
                "cannot watch the work tree; reading on a timer"
            );
            None
        }
    }
}

/// The work tree's path as given, and as the filesystem reports it.
///
/// Both, because macOS resolves symlinks in the paths it hands back: a repository
/// under `/var/folders/...` is reported under `/private/var/folders/...`. A path
/// that cannot be made relative to the tree is treated as "cannot tell, read it",
/// so getting this wrong does not break correctness — it silently turns the whole
/// filter off, which is the same as not having written it.
pub(super) struct Roots {
    given: PathBuf,
    canonical: PathBuf,
}

impl Roots {
    pub(super) fn of(root: &Path) -> Self {
        Self {
            given: root.to_path_buf(),
            canonical: root.canonicalize().unwrap_or_else(|_| root.to_path_buf()),
        }
    }

    fn relative<'a>(&self, path: &'a Path) -> Option<&'a Path> {
        path.strip_prefix(&self.canonical)
            .or_else(|_| path.strip_prefix(&self.given))
            .ok()
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
    let Some(relative) = roots.relative(path) else {
        // Outside the tree, which the watcher should not report. A path that
        // cannot be placed is read rather than dropped.
        return true;
    };
    if relative.starts_with(".git") {
        return git_metadata_matters(relative);
    }
    let Some(repo) = repo else {
        return true;
    };
    // Build output is the loudest thing in a working tree and the one thing git
    // has been told to disregard: a `cargo build` writes thousands of files that
    // cannot appear in a status. Skipping them is what makes this worth having,
    // since a pane running a build is nightcrow's ordinary state.
    //
    // A tracked file inside an ignored directory (added with `-f`) is the case
    // this skips wrongly. The idle read is what still catches it.
    !repo.is_path_ignored(relative).unwrap_or(false)
}

/// Whether a change under `.git` could change what a status says.
fn git_metadata_matters(relative: &Path) -> bool {
    // Objects and reflogs churn on every commit and every fetch, and neither
    // changes a status by itself — the index or ref update that comes with them
    // does, and that is watched.
    if relative.starts_with(".git/objects") || relative.starts_with(".git/logs") {
        return false;
    }
    // Git takes `index.lock` before an operation and removes it after. Reading
    // on that means reading a tree mid-change, and the real event follows
    // immediately.
    relative.extension().is_none_or(|ext| ext != "lock")
}

#[cfg(test)]
#[path = "snapshot_watch_tests.rs"]
mod tests;
