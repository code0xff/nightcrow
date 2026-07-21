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

use crate::web::viewer::dto::RepoDto;
use crate::web::viewer::runtime::RepoRuntime;
use crate::web::viewer::terminal::TerminalHub;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// One served repository: its identity, and the runtime streaming its status.
pub struct RepoEntry {
    pub id: String,
    /// Absolute worktree path. Server-side only — never serialized.
    pub path: String,
    pub name: String,
    pub display_path: String,
    pub runtime: Arc<RepoRuntime>,
    /// This repository's terminals. Independent of the TUI's panes — see
    /// [`crate::web::viewer::terminal`].
    pub terminals: Arc<TerminalHub>,
}

impl RepoEntry {
    pub fn to_dto(&self) -> RepoDto {
        RepoDto {
            id: self.id.clone(),
            name: self.name.clone(),
            display_path: self.display_path.clone(),
        }
    }
}

/// Hands out ids and never reuses one, so a path that leaves and comes back
/// keeps the identity a client already knows.
#[derive(Default)]
struct IdAssigner {
    next: u64,
    by_path: HashMap<String, String>,
}

impl IdAssigner {
    fn id_for(&mut self, path: &str) -> String {
        if let Some(existing) = self.by_path.get(path) {
            return existing.clone();
        }
        self.next += 1;
        let id = format!("r{}", self.next);
        self.by_path.insert(path.to_string(), id.clone());
        id
    }
}

/// Result of [`Catalog::add_path`].
pub enum AddOutcome {
    /// The repository is now served — newly added, or already present.
    Added(RepoDto),
    /// The served set is already at its ceiling; nothing was added.
    TooMany,
}

#[derive(Default)]
pub struct Catalog {
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

    /// The desired served set: base first, then browser-added, minus any path
    /// closed from the browser, deduplicated.
    fn union_paths(&self) -> Vec<String> {
        let base = self.base.lock().expect("catalog base poisoned");
        let added = self.added.lock().expect("catalog added poisoned");
        let hidden = self.hidden.lock().expect("catalog hidden poisoned");
        let mut union: Vec<String> = Vec::with_capacity(base.len() + added.len());
        for path in base.iter().chain(added.iter()) {
            if hidden.iter().any(|h| h == path) {
                continue;
            }
            if !union.contains(path) {
                union.push(path.clone());
            }
        }
        union
    }

    fn dto_for_path(&self, path: &str) -> Option<RepoDto> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.to_dto())
    }

    /// Reconcile the live entries to `union_paths()`.
    ///
    /// A path already present keeps its entry — and therefore its runtime and
    /// every SSE subscriber attached to it. Only genuinely new paths start a
    /// runtime, and only genuinely removed ones stop.
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

    pub fn list(&self) -> Vec<RepoDto> {
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
mod tests {
    use super::*;
    use crate::test_util::make_repo;

    #[test]
    fn ids_are_stable_across_catalog_updates() {
        let (dir_a, a) = make_repo();
        let (dir_b, b) = make_repo();
        let catalog = Catalog::new();

        catalog.set_paths(std::slice::from_ref(&a));
        let first = catalog.list()[0].id.clone();

        // Opening a second repository must not renumber the first.
        catalog.set_paths(&[a.clone(), b.clone()]);
        let after = catalog.list();

        assert_eq!(after[0].id, first);
        assert_ne!(after[1].id, first);
        catalog.shutdown();
        drop((dir_a, dir_b));
    }

    #[test]
    fn a_path_that_leaves_and_returns_keeps_its_id() {
        let (dir_a, a) = make_repo();
        let (dir_b, b) = make_repo();
        let catalog = Catalog::new();

        catalog.set_paths(&[a.clone(), b.clone()]);
        let a_id = catalog.list()[0].id.clone();
        catalog.set_paths(std::slice::from_ref(&b));
        catalog.set_paths(&[b.clone(), a.clone()]);

        let reopened = catalog.get(&a_id).expect("the id must still resolve");
        assert_eq!(reopened.path, a);
        catalog.shutdown();
        drop((dir_a, dir_b));
    }

    #[test]
    fn add_path_is_idempotent_and_respects_the_cap() {
        let (dir_a, a) = make_repo();
        let (dir_b, b) = make_repo();
        let (dir_c, c) = make_repo();
        let catalog = Catalog::new();

        let id_a = match catalog.add_path(a.clone(), 2) {
            AddOutcome::Added(dto) => dto.id,
            AddOutcome::TooMany => panic!("the first add must succeed"),
        };
        assert_eq!(catalog.len(), 1);

        // Re-adding an open path is a no-op that returns the same identity.
        match catalog.add_path(a.clone(), 2) {
            AddOutcome::Added(dto) => assert_eq!(dto.id, id_a),
            AddOutcome::TooMany => panic!("re-adding an open repo must not be refused"),
        }
        assert_eq!(catalog.len(), 1);

        // A second distinct repo fits under the cap of two.
        assert!(matches!(catalog.add_path(b, 2), AddOutcome::Added(_)));
        assert_eq!(catalog.len(), 2);

        // A third exceeds the cap and is refused without disturbing the set.
        assert!(matches!(catalog.add_path(c, 2), AddOutcome::TooMany));
        assert_eq!(catalog.len(), 2);

        catalog.shutdown();
        drop((dir_a, dir_b, dir_c));
    }

    #[test]
    fn a_browser_added_repo_survives_a_base_update() {
        let (dir_a, a) = make_repo();
        let (dir_b, b) = make_repo();
        let catalog = Catalog::new();

        catalog.set_paths(std::slice::from_ref(&a));
        let added = match catalog.add_path(b, 10) {
            AddOutcome::Added(dto) => dto.id,
            AddOutcome::TooMany => panic!("the add must succeed"),
        };

        // The TUI opening or closing a tab re-runs set_paths with a new base;
        // a repository opened from the browser must not be dropped by it.
        catalog.set_paths(std::slice::from_ref(&a));
        assert!(
            catalog.get(&added).is_some(),
            "a browser-added repo must survive a base update"
        );

        catalog.shutdown();
        drop((dir_a, dir_b));
    }

    #[test]
    fn remove_path_closes_and_stays_closed_until_reopened() {
        let (dir_a, a) = make_repo();
        let (dir_b, b) = make_repo();
        let catalog = Catalog::new();
        catalog.set_paths(&[a.clone(), b.clone()]);
        assert_eq!(catalog.len(), 2);

        catalog.remove_path(&a);
        assert_eq!(catalog.len(), 1);

        // A base re-sync (a TUI tab change) must not resurrect a closed repo.
        catalog.set_paths(&[a.clone(), b.clone()]);
        assert_eq!(catalog.len(), 1, "a closed repo must stay closed");

        // Re-opening it from the browser clears the close and brings it back.
        assert!(matches!(catalog.add_path(a.clone(), 10), AddOutcome::Added(_)));
        assert_eq!(catalog.len(), 2);

        catalog.shutdown();
        drop((dir_a, dir_b));
    }

    #[test]
    fn an_unchanged_path_keeps_its_runtime_and_subscribers() {
        let (dir_a, a) = make_repo();
        let (dir_b, b) = make_repo();
        let catalog = Catalog::new();
        catalog.set_paths(std::slice::from_ref(&a));

        let entry = catalog.get(&catalog.list()[0].id).unwrap();
        let _subscription = entry.runtime.subscribe();
        assert_eq!(entry.runtime.subscriber_count(), 1);

        // Adding an unrelated repository must not restart the existing one.
        catalog.set_paths(&[a.clone(), b.clone()]);

        let same = catalog.get(&entry.id).unwrap();
        assert!(
            Arc::ptr_eq(&same.runtime, &entry.runtime),
            "an unchanged path must not get a fresh runtime"
        );
        assert_eq!(
            same.runtime.subscriber_count(),
            1,
            "a catalog update must not drop live SSE clients"
        );
        catalog.shutdown();
        drop((dir_a, dir_b));
    }

    #[test]
    fn removing_a_path_drops_it_from_lookup() {
        let (dir_a, a) = make_repo();
        let catalog = Catalog::new();
        catalog.set_paths(std::slice::from_ref(&a));
        let id = catalog.list()[0].id.clone();

        catalog.set_paths(&[]);

        assert!(
            catalog.get(&id).is_none(),
            "a closed repo must stop resolving"
        );
        assert!(catalog.is_empty());
        drop(dir_a);
    }

    #[test]
    fn duplicate_paths_collapse_to_one_entry() {
        let (dir_a, a) = make_repo();
        let catalog = Catalog::new();

        catalog.set_paths(&[a.clone(), a.clone(), a.clone()]);

        assert_eq!(catalog.len(), 1, "one worktree is one entry");
        catalog.shutdown();
        drop(dir_a);
    }

    #[test]
    fn an_unknown_id_does_not_resolve() {
        let catalog = Catalog::new();
        assert!(catalog.get("r999").is_none());
        assert!(catalog.get("").is_none());
        assert!(catalog.get("../etc").is_none());
    }

    #[test]
    fn the_dto_exposes_only_the_whitelisted_identity_fields() {
        let (dir_a, a) = make_repo();
        let catalog = Catalog::new();
        catalog.set_paths(std::slice::from_ref(&a));

        let value = serde_json::to_value(&catalog.list()[0]).unwrap();

        let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(keys, vec!["display_path", "id", "name"]);
        catalog.shutdown();
        drop(dir_a);
    }

    #[test]
    fn display_path_abbreviates_the_home_directory() {
        // A repo under $HOME is sent home-relative, so the payload does not
        // carry the account name. A repo outside it has nothing to abbreviate —
        // the path is the only label that identifies it to the user, and the
        // client is already authenticated to a session that has a shell.
        let home = dirs::home_dir().expect("a home directory");

        assert_eq!(
            display_path(&home.join("code").join("app").to_string_lossy()),
            "~/code/app"
        );
        assert_eq!(display_path(&home.to_string_lossy()), "~");
        assert_eq!(display_path("/opt/elsewhere"), "/opt/elsewhere");
        assert_eq!(
            display_path("/opt/elsewhere/"),
            "/opt/elsewhere",
            "libgit2's trailing separator must not reach the UI"
        );
    }

    #[test]
    fn repo_name_ignores_a_trailing_separator() {
        assert_eq!(repo_name("/code/app/"), "app");
        assert_eq!(repo_name("/code/app"), "app");
    }
}
