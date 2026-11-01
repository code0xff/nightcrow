//! Which watcher events can change what a status says.
//!
//! Separated from the watch itself because this is the part that asks git, and
//! the answer is cached per read cycle: a build is thousands of events under a
//! directory `.gitignore` already excludes.

use super::{Roots, git_metadata_matters};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Whether any of `paths` could change what a status says.
///
/// `repo` is used to ask git what it ignores; without a handle open every path
/// counts, since guessing wrong the other way would leave a real change unread.
/// `ignored` remembers the directory prefixes that answered yes, so the rest of
/// a build's event burst does not repeat the same lookup.
pub(in crate::runtime) fn any_matters(
    repo: Option<&git2::Repository>,
    roots: &Roots,
    paths: &[PathBuf],
    ignored: &mut IgnoredDirs,
) -> bool {
    // Nothing named: the watcher lost track, so look.
    if paths.is_empty() {
        return true;
    }
    ignored.forget_if_rules_change(paths);
    paths.iter().any(|path| matters(repo, roots, path, ignored))
}

/// The ignored directory prefixes already seen since the last read.
///
/// A build is thousands of events under one directory that `.gitignore` already
/// excludes, and answering each one with a libgit2 lookup made the filter as
/// expensive as the read it exists to avoid. Only directories are remembered:
/// a file ignored by name says nothing about its siblings.
#[derive(Default)]
pub(in crate::runtime) struct IgnoredDirs {
    prefixes: HashSet<PathBuf>,
}

impl IgnoredDirs {
    /// Drop what is known about the rules when an event can have changed them.
    ///
    /// Called for every batch, so a `.gitignore` edit wakes the reader instead of
    /// being filtered by the rules it just replaced.
    fn forget_if_rules_change(&mut self, paths: &[PathBuf]) {
        let rules_changed = paths
            .iter()
            .any(|path| path.file_name() == Some(OsStr::new(".gitignore")));
        if rules_changed {
            self.clear();
        }
    }

    pub(in crate::runtime) fn clear(&mut self) {
        self.prefixes.clear();
    }

    pub(super) fn covers(&self, relative: &Path) -> bool {
        relative
            .ancestors()
            .skip(1)
            .any(|dir| self.prefixes.contains(dir))
    }

    fn remember(&mut self, prefix: &Path) {
        self.prefixes.insert(prefix.to_path_buf());
    }
}

fn matters(
    repo: Option<&git2::Repository>,
    roots: &Roots,
    path: &Path,
    ignored: &mut IgnoredDirs,
) -> bool {
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
    if ignored.covers(relative) {
        return false;
    }
    // Build output is the loudest thing in a working tree and the one thing git
    // has been told to disregard: skipping it is what makes this worth having.
    //
    // A tracked file inside an ignored directory (added with `-f`) is the case
    // this skips wrongly. The idle read is what still catches it.
    if !repo.is_path_ignored(relative).unwrap_or(false) {
        return true;
    }
    if let Some(prefix) = ignored_prefix(repo, relative) {
        ignored.remember(prefix);
    }
    false
}

/// The shallowest directory above `relative` that git itself calls ignored.
///
/// The shallowest, so one lookup covers the widest set of siblings; the event
/// path's own ancestors are tested only after git has already said the path is
/// ignored. A trailing `/` makes libgit2 treat the probe as a directory without
/// needing it to still exist — a build's removed directory is exactly when the
/// remaining events arrive.
fn ignored_prefix<'a>(repo: &git2::Repository, relative: &'a Path) -> Option<&'a Path> {
    let mut dirs: Vec<&Path> = relative
        .ancestors()
        .skip(1)
        .filter(|dir| !dir.as_os_str().is_empty())
        .collect();
    dirs.reverse();
    dirs.into_iter()
        .find(|dir| repo.is_path_ignored(directory_probe(dir)).unwrap_or(false))
}

fn directory_probe(dir: &Path) -> PathBuf {
    let mut probe = dir.as_os_str().to_os_string();
    probe.push("/");
    PathBuf::from(probe)
}
