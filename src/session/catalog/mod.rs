//! The set of repositories the viewer serves, and their runtimes.
//!
//! Clients address a repository by an opaque `id`, never by path — a request
//! cannot name a directory, only pick one the server already decided to expose.
//! Ids are stable for the process lifetime, so opening or closing an unrelated
//! tab does not renumber the others.
//!
//! Replacement is atomic and does no blocking work under the lock: the new list
//! is built, swapped in, and only then are the dropped runtimes stopped.

use crate::session::StatusEncoder;
use crate::session::runtime::RepoRuntime;
use crate::session::terminal::TerminalHub;
use std::path::Path;
use std::sync::{Arc, Mutex};

mod catalog_ids;
mod config_tables;
mod ordering;
use catalog_ids::IdAssigner;
pub use catalog_ids::{AddOutcome, RepoEntry, RepoInfo};

#[derive(Default)]
pub struct Catalog {
    mutation: Mutex<()>,
    entries: Mutex<Vec<Arc<RepoEntry>>>,
    ids: Mutex<IdAssigner>,
    /// Repositories supplied by the CLI (`serve --repo`) or pushed from the TUI
    /// workspace. Replaced wholesale by [`Catalog::set_paths`].
    base: Mutex<Vec<String>>,
    /// Repositories opened from the browser. Kept across `base` updates.
    added: Mutex<Vec<String>>,
    /// Repositories closed from the browser. Subtracted from the served set so
    /// a `base` re-sync does not resurrect a closed repo.
    hidden: Mutex<Vec<String>>,
    order: Mutex<Vec<String>>,
    /// Commands each repository's terminal hub runs as startup terminals on the
    /// first client connect. Behind a lock because a config reload replaces it;
    /// only hubs spawned *after* the reload see the new list.
    startup_commands: Mutex<Vec<crate::config::StartupCommand>>,
    /// The `--exec` panes the daemon was started with, appended after the
    /// configured ones. Not behind a lock: these came from the command line.
    cli_startup: Vec<String>,
    /// The `[[plugin]]` table, handed to every hub the catalog spawns. Replaced
    /// by a reload; the hubs already running are told as well, because a plugin
    /// is a child process and restarting one costs the session nothing.
    plugins: Mutex<Vec<crate::config::PluginConfig>>,
    /// The shell every terminal pane is spawned with. Fixed for the session's
    /// life: a config reload does not replace the shell of a running hub.
    shell: crate::config::ShellConfig,
    /// Which screen this session's panes are fitted to, shared by every hub.
    /// One value for the session rather than one per repository — see
    /// [`crate::session::size_owner`].
    ownership: Arc<crate::session::size_owner::SizeOwnership>,
    /// Surface-owned status representation cached by each repository runtime.
    status_encoder: Option<StatusEncoder>,
}

impl Catalog {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
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
        match self.info_for_path(&path) {
            Some(info) => AddOutcome::Added(info),
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

    fn info_for_path(&self, path: &str) -> Option<RepoInfo> {
        self.entries
            .lock()
            .expect("catalog poisoned")
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.info())
    }

    /// Reconcile the live entries to `union_paths()`. A path already present
    /// keeps its entry — and therefore its runtime and every SSE subscriber
    /// attached to it. Only genuinely new paths start a runtime, and only
    /// genuinely removed ones stop.
    fn rebuild(&self) {
        let deduped = self.union_paths();
        // Read once, before the entries lock: every hub this pass spawns is
        // given the same tables, so a reload landing mid-rebuild cannot leave two
        // repositories opened in the same beat configured differently.
        let startup = self
            .startup_commands
            .lock()
            .expect("catalog startup poisoned")
            .clone();
        let plugins = self
            .plugins
            .lock()
            .expect("catalog plugins poisoned")
            .clone();

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
                        runtime: RepoRuntime::spawn(
                            &path,
                            self.status_encoder.unwrap_or(empty_status_payload),
                        ),
                        terminals: TerminalHub::spawn(
                            &path,
                            startup.clone(),
                            plugins.clone(),
                            self.shell.clone(),
                            Arc::clone(&self.ownership),
                        ),
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

    /// Stop every runtime. Called on server shutdown.
    pub fn shutdown(&self) {
        let entries = std::mem::take(&mut *self.entries.lock().expect("catalog poisoned"));
        for entry in entries {
            entry.runtime.stop();
            entry.terminals.stop();
        }
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
