//! Reading the served set: the projections each surface needs of it.
//!
//! Each snapshots the same map under one lock. Reading it in two calls lets a
//! repository opened in between appear in one and not the other.

use super::{Catalog, RepoEntry, RepoInfo};
use std::collections::HashMap;
use std::sync::Arc;

/// One snapshot of the served set, in the three shapes a response needs it in.
pub struct ServedView {
    pub list: Vec<RepoInfo>,
    /// Id standing for the remembered project, when it is served.
    pub active: Option<String>,
    /// Which panel each served project was left maximized in, by id.
    pub maximized: HashMap<String, crate::session::prefs::MaximizedPanel>,
    /// What each served project was last showing, by id.
    pub views: HashMap<String, crate::session::prefs::RepoView>,
}

impl Catalog {
    pub fn get(&self, id: &str) -> Option<Arc<RepoEntry>> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .find(|e| e.id == id)
            .map(Arc::clone)
    }

    /// Every served entry, for a caller that needs the runtimes themselves
    /// rather than a client-facing projection.
    ///
    /// A snapshot: the `Arc`s are cloned out and the lock released.
    pub fn entries(&self) -> Vec<Arc<RepoEntry>> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .map(Arc::clone)
            .collect()
    }

    /// The served list and, from that same snapshot, the id standing for
    /// `remembered`.
    ///
    /// One lock for both, because a client renders them together: a repository
    /// opened between two separate reads would yield an active id missing from
    /// the list beside it.
    pub fn list_with_active(
        &self,
        remembered: Option<&str>,
        maximized: &[crate::session::prefs::RepoMaximized],
        views: &[crate::session::prefs::RepoView],
    ) -> ServedView {
        let entries = self.entries.lock().expect("catalog poisoned");
        let list = entries.iter().map(|e| e.info()).collect();
        let active = remembered.and_then(|path| {
            entries
                .iter()
                .find(|e| e.path == path)
                .map(|e| e.id.clone())
        });
        // From the same snapshot for the same reason: a repository opened
        // between two reads would be in the list with no arrangement beside it,
        // or have one under an id the list does not carry.
        let arrangements = entries
            .iter()
            .filter_map(|e| {
                crate::session::prefs::maximized::panel_of(maximized, &e.path)
                    .map(|panel| (e.id.clone(), panel))
            })
            .collect();
        // And the same again for what each was showing. A project the session
        // is not serving keeps its entry on file — there is no id to name it by
        // here, and it will want it back when it is opened.
        let last_views = entries
            .iter()
            .filter_map(|e| {
                crate::session::prefs::repo_view::view_of(views, &e.path)
                    .map(|view| (e.id.clone(), view.clone()))
            })
            .collect();
        ServedView {
            list,
            active,
            maximized: arrangements,
            views: last_views,
        }
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

    pub fn list(&self) -> Vec<RepoInfo> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .map(|e| e.info())
            .collect()
    }

    /// Ids paired with absolute worktree paths, in order.
    ///
    /// For the attach transport, whose clients read those paths from the same
    /// filesystem the daemon is on. The browser gets [`RepoDto`] instead, which
    /// carries a home-relative path for display and no absolute one — but the
    /// browser's own response builder reads this too, to turn a preference
    /// stored by path back into the ids it speaks.
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

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.lock().expect("catalog poisoned").len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
