//! The set of repositories the viewer serves, and their runtimes.
//!
//! Clients address a repository by an opaque `id`, never by path. That is the
//! whole point: a request cannot name a directory, only pick one the server
//! already decided to expose, so "which repository" is not an input to
//! validate — it is a lookup that either hits or 404s.
//!
//! Ids are stable for the process lifetime. A path keeps its id across catalog
//! updates, so opening or closing an unrelated tab does not renumber the others
//! and invalidate a client's bookmarks mid-session.
//!
//! Replacement is atomic and does no blocking work under the lock: the new list
//! is built, swapped in, and only then are the dropped runtimes stopped — a
//! runtime shutdown joins a thread, and holding the catalog lock across that
//! would stall every in-flight request.

use crate::web::viewer::runtime::RepoRuntime;
use crate::web::viewer::terminal::TerminalHub;
use std::path::Path;
use std::sync::{Arc, Mutex};

mod catalog_ids;
mod ordering;
use catalog_ids::IdAssigner;
pub use catalog_ids::{AddOutcome, RepoEntry};

#[derive(Default)]
pub struct Catalog {
    mutation: Mutex<()>,
    entries: Mutex<Vec<Arc<RepoEntry>>>,
    ids: Mutex<IdAssigner>,
    /// Repositories supplied by the CLI (`serve --repo`) or pushed from the TUI
    /// workspace. Replaced wholesale by [`Catalog::set_paths`].
    base: Mutex<Vec<String>>,
    /// Repositories opened from the browser. Kept across `base` updates so a
    /// workspace tab change in the TUI does not drop them.
    added: Mutex<Vec<String>>,
    /// Repositories closed from the browser. Subtracted from the served set so
    /// a `base` re-sync (a TUI tab change) does not resurrect a closed repo;
    /// re-opening a path clears it from here.
    hidden: Mutex<Vec<String>>,
    order: Mutex<Vec<String>>,
    /// Commands each repository's terminal hub runs as startup terminals on the
    /// first client connect (empty = one bare shell). Applied to every hub the
    /// catalog spawns.
    startup_commands: Vec<String>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Like [`Catalog::new`], but every terminal hub it spawns runs `startup`
    /// as its startup terminals.
    pub fn with_startup(startup_commands: Vec<String>) -> Self {
        Self {
            startup_commands,
            ..Self::default()
        }
    }

    /// Replace the base served set — CLI `--repo` args or the TUI workspace's
    /// open tabs. Browser-opened repositories ([`Catalog::add_path`]) survive
    /// this, so a workspace change does not close a tab a viewer opened.
    pub fn set_paths(&self, paths: &[String]) {
        let _mutation = self.mutation.lock().expect("catalog mutation poisoned");
        {
            let mut base = self.base.lock().expect("catalog base poisoned");
            *base = paths.to_vec();
        }
        self.rebuild();
    }

    /// Add a repository opened from the browser, returning its identity.
    ///
    /// Idempotent: an already-served path returns its existing entry without
    /// disturbing its runtime. Refused with [`AddOutcome::TooMany`] once the
    /// served set is at `max`, so a client cannot spawn unbounded runtimes.
    pub fn add_path(&self, path: String, max: usize) -> AddOutcome {
        let _mutation = self.mutation.lock().expect("catalog mutation poisoned");
        // Opening a path clears any prior close, so a previously removed repo
        // comes back rather than staying suppressed by `hidden`.
        {
            let mut hidden = self.hidden.lock().expect("catalog hidden poisoned");
            hidden.retain(|h| h != &path);
        }
        let union = self.union_paths();
        if !union.iter().any(|p| p == &path) {
            if union.len() >= max {
                return AddOutcome::TooMany;
            }
            {
                let mut added = self.added.lock().expect("catalog added poisoned");
                if !added.iter().any(|p| p == &path) {
                    added.push(path.clone());
                }
            }
        }
        self.rebuild();
        match self.dto_for_path(&path) {
            Some(dto) => AddOutcome::Added(dto),
            // rebuild always creates the entry; this only trips if a concurrent
            // set_paths raced it back out, which the caller can treat as full.
            None => AddOutcome::TooMany,
        }
    }

    /// Close a repository opened or shown in the browser. Removed from `added`
    /// and remembered in `hidden` so a `base` re-sync will not bring it back;
    /// `rebuild` then stops its runtime and terminals.
    pub fn remove_path(&self, path: &str) {
        let _mutation = self.mutation.lock().expect("catalog mutation poisoned");
        {
            let mut added = self.added.lock().expect("catalog added poisoned");
            added.retain(|p| p != path);
        }
        {
            let mut hidden = self.hidden.lock().expect("catalog hidden poisoned");
            if !hidden.iter().any(|h| h == path) {
                hidden.push(path.to_string());
            }
        }
        self.rebuild();
    }

    fn dto_for_path(&self, path: &str) -> Option<crate::web::viewer::dto::RepoDto> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.to_dto())
    }

    /// Reconcile the live entries to `union_paths()`. A path already present
    /// keeps its entry — and therefore its runtime and every SSE subscriber
    /// attached to it. Only genuinely new paths start a runtime, and only
    /// genuinely removed ones stop.
    fn rebuild(&self) {
        let deduped = self.union_paths();

        let assigned: Vec<(String, String)> = {
            let mut ids = self.ids.lock().expect("catalog ids poisoned");
            deduped
                .iter()
                .map(|path| (ids.id_for(path), path.clone()))
                .collect()
        };

        let retired = {
            let mut entries = self.entries.lock().expect("catalog poisoned");
            let previous = std::mem::take(&mut *entries);

            let mut next = Vec::with_capacity(assigned.len());
            for (id, path) in assigned {
                match previous.iter().find(|e| e.path == path) {
                    Some(existing) => next.push(Arc::clone(existing)),
                    None => next.push(Arc::new(RepoEntry {
                        name: repo_name(&path),
                        display_path: display_path(&path),
                        runtime: RepoRuntime::spawn(&path),
                        terminals: TerminalHub::spawn(&path, self.startup_commands.clone()),
                        id,
                        path,
                    })),
                }
            }

            let retired: Vec<_> = previous
                .into_iter()
                .filter(|old| !next.iter().any(|new| Arc::ptr_eq(new, old)))
                .collect();
            *entries = next;
            retired
        };

        // Outside the lock: stopping a runtime joins its thread.
        for entry in retired {
            entry.runtime.stop();
            entry.terminals.stop();
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<RepoEntry>> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .find(|e| e.id == id)
            .map(Arc::clone)
    }

    /// The id currently standing for `path`, or `None` when that path is not
    /// served. The inverse of [`Catalog::get`], for the one caller that stores
    /// a repository across restarts (`prefs.rs`) and so cannot hold an id.
    pub fn id_of_path(&self, path: &str) -> Option<String> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.id.clone())
    }

    pub fn list(&self) -> Vec<crate::web::viewer::dto::RepoDto> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .map(|e| e.to_dto())
            .collect()
    }

    /// Absolute worktree paths of the served set, in order. Used to persist the
    /// open projects; never serialized to a client.
    pub fn paths(&self) -> Vec<String> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .map(|e| e.path.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().expect("catalog poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Stop every runtime. Called on server shutdown.
    pub fn shutdown(&self) {
        let entries = std::mem::take(&mut *self.entries.lock().expect("catalog poisoned"));
        for entry in entries {
            entry.runtime.stop();
            entry.terminals.stop();
        }
    }
}

fn repo_name(path: &str) -> String {
    Path::new(path.trim_end_matches('/'))
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Render `path` with the home directory as `~`, so the client shows a short
/// path and never learns the account name.
fn display_path(path: &str) -> String {
    // libgit2 hands back a workdir with a trailing separator; a path shown to
    // a person should not carry it.
    let path = path.trim_end_matches('/');
    let Some(home) = dirs::home_dir() else {
        return path.to_string();
    };
    match Path::new(path).strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.to_string(),
    }
}

#[cfg(test)]
mod catalog_tests;
