//! The set of repositories the viewer serves, and their runtimes.
//!
//! Clients address a repository by an opaque `id`, never by path — a request
//! cannot name a directory, only pick one the server already decided to expose.
//! Ids are stable for the process lifetime, so opening or closing an unrelated
//! tab does not renumber the others.
//!
//! Replacement is atomic and does no blocking work under the lock: the new list
//! is built and swapped in, and only then are the dropped runtimes stopped — a
//! runtime shutdown joins a thread, and holding the catalog lock across that
//! would stall every in-flight request.
//!
//! **Every path in here is the one `resolve_repo_path` produces**, normalised on
//! the way in rather than by each caller. Two spellings of one worktree are two
//! strings, and the whole catalog decides identity by comparing them, so a path
//! that arrived spelled differently opened a second tab on a repository already
//! open. Holding the invariant at the boundary keeps every entry point from
//! having to remember it.

use std::path::Path;
use std::sync::{Arc, Mutex};

mod catalog_ids;
mod catalog_runtime;
mod config_tables;
mod membership;
mod ordering;
pub use catalog_ids::{AddOutcome, RepoEntry, RepoInfo};
use catalog_runtime::CatalogRuntime;
use membership::{AddMembership, CatalogMembership};

pub struct Catalog {
    /// Serializes membership-to-runtime commits and config swaps. The two
    /// subobjects have independent locks so read-only runtime snapshots do not
    /// need the membership bookkeeping, but a mutation always crosses them as
    /// one facade transaction.
    transaction: Mutex<()>,
    membership: Mutex<CatalogMembership>,
    runtime: Mutex<CatalogRuntime>,
}

impl Default for Catalog {
    fn default() -> Self {
        Self {
            transaction: Mutex::new(()),
            membership: Mutex::new(CatalogMembership::default()),
            runtime: Mutex::new(CatalogRuntime::default()),
        }
    }
}

impl Catalog {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// One worktree's single spelling: what `git` calls the working directory,
    /// or the canonical directory when there is no repository there.
    ///
    /// Applied to everything entering the catalog, even though `add_path`'s
    /// caller resolves too — doing it twice costs one `discover`, cheap next
    /// to this invariant depending on every caller having remembered.
    fn normalized(path: &str) -> String {
        crate::git::resolve_repo_path(Path::new(path))
            .to_string_lossy()
            .into_owned()
    }

    /// Replace the base served set — CLI `--repo` args or the TUI workspace's
    /// open tabs. Browser-opened repositories ([`Catalog::add_path`]) survive
    /// this, so a workspace change does not close a tab a viewer opened.
    pub fn set_paths(&self, paths: &[String]) {
        let paths = paths.iter().map(|path| Self::normalized(path)).collect();
        self.change_membership(|membership| membership.set_paths(paths));
    }

    /// Add a repository opened from the browser, returning its identity.
    ///
    /// Idempotent: an already-served path returns its existing entry without
    /// disturbing its runtime. Refused with [`AddOutcome::TooMany`] once the
    /// served set is at `max`, so a client cannot spawn unbounded runtimes.
    pub fn add_path(&self, path: String, max: usize) -> AddOutcome {
        let path = Self::normalized(&path);
        let (outcome, retired) = {
            let _transaction = self
                .transaction
                .lock()
                .expect("catalog transaction poisoned");
            let mut membership = self.membership.lock().expect("catalog membership poisoned");
            let id = match membership.add_path(path, max) {
                AddMembership::Present(id) => id,
                AddMembership::TooMany => return AddOutcome::TooMany,
            };
            let members = membership.members();
            drop(membership);
            let mut runtime = self.runtime.lock().expect("catalog runtime poisoned");
            let retired = runtime.reconcile(members);
            let info = runtime
                .entries()
                .iter()
                .find(|entry| entry.id == id)
                .expect("accepted membership is committed to the runtime")
                .info();
            (AddOutcome::Added(info), retired)
        };
        stop_entries(retired);
        outcome
    }

    /// Close a repository opened or shown in the browser. Dropped from every
    /// list that decides the served set and remembered in `hidden` so a `base`
    /// re-sync will not bring it back; the facade transaction then retires its
    /// runtime and terminals.
    ///
    /// A close forgets the slot the repository held, `base` and `order`
    /// included. Leaving it in either meant [`Catalog::add_path`] found the
    /// path already in `union_paths` and never appended it, so re-opening put
    /// the tab back in the middle of the strip rather than at the end.
    pub fn remove_path(&self, path: &str) {
        let path = &Self::normalized(path);
        self.change_membership(|membership| membership.remove_path(path));
    }

    fn change_membership(&self, change: impl FnOnce(&mut CatalogMembership)) {
        self.change_membership_if(|membership| {
            change(membership);
            true
        });
    }

    fn change_membership_if(&self, change: impl FnOnce(&mut CatalogMembership) -> bool) -> bool {
        let (changed, retired) = {
            let _transaction = self
                .transaction
                .lock()
                .expect("catalog transaction poisoned");
            let mut membership = self.membership.lock().expect("catalog membership poisoned");
            let changed = change(&mut membership);
            if !changed {
                return false;
            }
            let members = membership.members();
            drop(membership);
            let retired = self
                .runtime
                .lock()
                .expect("catalog runtime poisoned")
                .reconcile(members);
            (true, retired)
        };
        stop_entries(retired);
        changed
    }

    /// Stop every runtime. Called on server shutdown.
    pub fn shutdown(&self) {
        let retired = {
            let _transaction = self
                .transaction
                .lock()
                .expect("catalog transaction poisoned");
            self.runtime
                .lock()
                .expect("catalog runtime poisoned")
                .take_entries()
        };
        stop_entries(retired);
    }
}

fn stop_entries(entries: Vec<Arc<RepoEntry>>) {
    for entry in entries {
        entry.runtime.stop();
        entry.terminals.stop();
    }
}

fn empty_status_payload(
    _: &crate::git::diff::RepoSnapshot,
    _: &std::collections::HashMap<String, std::time::SystemTime>,
) -> Option<String> {
    Some("{}".to_string())
}

pub(super) fn repo_name(path: &str) -> String {
    Path::new(path.trim_end_matches('/'))
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Render `path` with the home directory as `~`, so the client shows a short
/// path and never learns the account name.
pub(super) fn display_path(path: &str) -> String {
    // libgit2 hands back a workdir with a trailing separator; a path shown to
    // a person should not carry it.
    let path = path.trim_end_matches('/');
    let display = crate::platform::paths::for_display(std::path::Path::new(path));
    let Some(home) = dirs::home_dir() else {
        return display.into_owned();
    };
    // Normalise the home directory to forward slashes so strip_prefix works
    // against the already-normalised display path on Windows.
    let home_display = crate::platform::paths::for_display(&home);
    match std::path::Path::new(display.as_ref())
        .strip_prefix(std::path::Path::new(home_display.as_ref()))
    {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => {
            // Use the string representation directly so the separator stays `/`
            // on Windows — `Path::display()` would re-introduce backslashes.
            let rest_str = rest.to_string_lossy();
            format!("~/{}", rest_str)
        }
        Err(_) => display.into_owned(),
    }
}

mod views;

#[cfg(test)]
mod catalog_tests;
