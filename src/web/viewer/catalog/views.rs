//! Reading the served set: the projections each surface needs of it.
//!
//! Each snapshots the same map under one lock. Reading it in two calls lets a
//! repository opened in between appear in one and not the other — and a client
//! handed a selection that is not in the list it was sent falls back and records
//! the fallback, overwriting what was remembered.

use super::{Catalog, RepoEntry};
use std::sync::Arc;

impl Catalog {
    pub fn get(&self, id: &str) -> Option<Arc<RepoEntry>> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .find(|e| e.id == id)
            .map(Arc::clone)
    }

    /// The served list and, from that same snapshot, the id standing for
    /// `remembered`.
    ///
    /// One lock for both, because a client renders them together: a repository
    /// opened between two separate reads would yield an active id that is
    /// missing from the list beside it, and the client — seeing a selection it
    /// cannot show — would fall back to its first tab and record *that*,
    /// dropping the remembered project for good.
    pub fn list_with_active(
        &self,
        remembered: Option<&str>,
    ) -> (Vec<crate::web::viewer::dto::RepoDto>, Option<String>) {
        let entries = self.entries.lock().expect("catalog poisoned");
        let list = entries.iter().map(|e| e.to_dto()).collect();
        let active = remembered.and_then(|path| {
            entries
                .iter()
                .find(|e| e.path == path)
                .map(|e| e.id.clone())
        });
        (list, active)
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

    /// Ids paired with absolute worktree paths, in order.
    ///
    /// For the attach transport, whose clients read those paths from the same
    /// filesystem the daemon is on. The browser gets [`RepoDto`] instead, which
    /// carries a home-relative path for display and no absolute one.
    pub fn id_paths(&self) -> Vec<(String, String)> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .map(|e| (e.id.clone(), e.path.clone()))
            .collect()
    }

    /// Absolute worktree paths of the served set, in order. Used to persist the
    /// open projects.
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
}
