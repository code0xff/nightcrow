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

#[derive(Default)]
pub struct Catalog {
    entries: Mutex<Vec<Arc<RepoEntry>>>,
    ids: Mutex<IdAssigner>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the served set with exactly `paths`.
    ///
    /// A path already present keeps its entry — and therefore its runtime and
    /// every SSE subscriber attached to it. Only genuinely new paths start a
    /// runtime, and only genuinely removed ones stop.
    pub fn set_paths(&self, paths: &[String]) {
        let mut deduped: Vec<String> = Vec::with_capacity(paths.len());
        for path in paths {
            if !deduped.contains(path) {
                deduped.push(path.clone());
            }
        }

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
                        terminals: TerminalHub::spawn(&path),
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
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Render `path` with the home directory as `~`, so the client shows a short
/// path and never learns the account name.
fn display_path(path: &str) -> String {
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
    }
}
