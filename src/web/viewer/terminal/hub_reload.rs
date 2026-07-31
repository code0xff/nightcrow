//! Re-applying the `[[plugin]]` table on a hub that is already running.
//!
//! A plugin is a child process rather than a pane, so unlike a startup terminal
//! it can be replaced without costing the session anything a person was using.
//! That is the whole reason a config reload reaches this far: the panes stay,
//! their processes stay, and only the thing watching them is rebuilt.
//!
//! Three rules the diff below is written around.
//!
//! **The opt-ins are this hub's, not the new file's.** A hub creates its startup
//! panes once for its life, so a `[[startup_command]]` added by the edit has no
//! pane here and will not get one — launching the plugin it names would be a
//! child process with nothing it could ever be given. What decides is the list
//! this hub was spawned with ([`TerminalHub::startup_commands`]).
//!
//! **A plugin that is watching something stays.** Removing a pane's opt-in from
//! the file does not remove the pane, and silently un-watching a live agent
//! terminal is worse than keeping a host the file no longer asks for. Turning
//! `enabled` off is the way to say stop; that is honoured.
//!
//! **The guard is never rebuilt.** Its relaunch budget is keyed by a pane's
//! token, which is what bounds a plugin that answers every exit with another
//! relaunch. Rebuilding it here would hand out a fresh allowance on every
//! reload, so the ceiling would never be reached — the same reasoning as
//! [`Plugins::take_over`](super::hub_plugins::Plugins::take_over).

use super::TerminalHub;
use super::hub_helpers::Command;
use super::hub_plugins::Plugins;
use crate::backend::{PaneId, PtyBackend};
use crate::config::PluginConfig;
use std::collections::{BTreeSet, HashMap};

/// What a reload did to one hub's plugins, for the log and the caller's report.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct PluginReload {
    /// Launched: newly enabled, or newly reachable.
    pub(super) started: Vec<String>,
    /// Stopped for good, and their panes released.
    pub(super) stopped: Vec<String>,
    /// Same plugin, new process — its command, arguments or environment changed.
    pub(super) restarted: Vec<String>,
    /// Panes whose held relaunch was given up because the plugin holding it is
    /// gone. The caller tells the clients, which are counting down to a deadline
    /// nothing will act on.
    pub(super) retired: Vec<PaneId>,
}

impl PluginReload {
    pub(super) fn is_empty(&self) -> bool {
        self.started.is_empty() && self.stopped.is_empty() && self.restarted.is_empty()
    }
}

impl TerminalHub {
    /// Ask the worker to re-apply `plugins`.
    ///
    /// Queued rather than done here: a plugin can drive a pane's keyboard, so
    /// every plugin host is worker-local and the command queue is the only way in
    /// (see [`super::hub_plugins`]). A full queue drops the request and says so —
    /// a hub that far behind is being hammered by a client, and blocking a
    /// reload behind it would hold up every other repository in the session.
    pub(crate) fn reload_plugins(&self, plugins: Vec<PluginConfig>) {
        if self
            .commands
            .try_send(Command::ReloadPlugins { plugins })
            .is_err()
        {
            tracing::warn!("viewer: a hub's queue was full; its plugins were not reloaded");
        }
    }

    /// Carry out a queued reload on the worker thread.
    ///
    /// The pane titles are read here rather than inside the diff because they
    /// live in the hub's shared state: a plugin that is (re)started is handed the
    /// panes it owns again, and a pane announced without its title would show up
    /// under a different name than the one the operator sees.
    pub(super) fn reload_hub_plugins(
        &self,
        backend: &mut PtyBackend,
        plugins: &mut Plugins,
        configs: &[PluginConfig],
    ) {
        let titles: HashMap<PaneId, Option<String>> = self
            .state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .map(|pane| (pane.id, pane.title.clone()))
            .collect();
        let outcome = plugins.reload(backend, configs, self.startup_commands(), &titles);
        // Told to the clients, or one that was shown a deadline keeps counting
        // down to a moment nothing will act on.
        for pane in &outcome.retired {
            self.end_recovery(*pane);
        }
        if !outcome.is_empty() {
            tracing::info!(
                started = ?outcome.started,
                stopped = ?outcome.stopped,
                restarted = ?outcome.restarted,
                retired = outcome.retired.len(),
                "viewer: re-applied the plugin table on a hub"
            );
        }
    }
}

impl Plugins {
    /// Bring the running hosts in line with `configs`.
    ///
    /// `startup` is the hub's own startup list — see this module's header for why
    /// it is not the reloaded one.
    pub(super) fn reload(
        &mut self,
        backend: &mut PtyBackend,
        configs: &[PluginConfig],
        startup: &[crate::config::StartupCommand],
        titles: &HashMap<PaneId, Option<String>>,
    ) -> PluginReload {
        let mut outcome = PluginReload::default();
        let wanted: Vec<&PluginConfig> = configs
            .iter()
            .filter(|cfg| self.is_wanted(cfg, startup))
            .collect();

        // Stop first, so a plugin being replaced has released its pipes before
        // its successor is spawned, and so two children of the same plugin are
        // never live at once.
        let keep: BTreeSet<&str> = wanted
            .iter()
            .filter(|cfg| !self.spec_changed(cfg))
            .map(|cfg| cfg.name.as_str())
            .collect();
        let leaving: Vec<String> = self
            .hosts
            .keys()
            .filter(|name| !keep.contains(name.as_str()))
            .cloned()
            .collect();
        for name in leaving {
            let replaced = wanted.iter().any(|cfg| cfg.name == name);
            self.stop_host(backend, &name, replaced, &mut outcome);
        }

        for cfg in wanted {
            if self.hosts.contains_key(&cfg.name) {
                // Kept as it was: only its rules may have changed, and those are
                // read from the maps below rather than from the child.
                self.allowed_flags
                    .insert(cfg.name.clone(), cfg.allowed_resume_flags.clone());
                self.set_watch_on_signal(&cfg.name, cfg.watch_on_signal);
                continue;
            }
            // A child that never came up leaves nothing behind. When this was a
            // replacement, `stop_host` kept the live panes for a successor that
            // has now failed to arrive, and an owner with no host is worse than
            // no owner at all — see [`Plugins::abandon`].
            if !self.start_host(cfg, backend, titles, &mut outcome) {
                self.abandon(backend, &cfg.name, &mut outcome);
            }
        }
        outcome
    }

    /// Whether `cfg` should have a host on this hub. See the module header for
    /// the third arm — a plugin already watching a pane is kept whatever the
    /// file now says about opt-ins.
    fn is_wanted(&self, cfg: &PluginConfig, startup: &[crate::config::StartupCommand]) -> bool {
        if !cfg.enabled {
            return false;
        }
        let opted_in = startup
            .iter()
            .any(|sc| sc.plugin.as_deref() == Some(cfg.name.as_str()));
        opted_in || cfg.watch_on_signal || self.owners.values().any(|owner| owner == &cfg.name)
    }

    /// Whether a live host's child would have to be replaced to match `cfg`.
    ///
    /// Only what the process was launched from counts. `allowed_resume_flags` and
    /// `watch_on_signal` are read from this side on every judgement, so tightening
    /// either takes effect without disturbing a plugin that may be hours into a
    /// wait.
    fn spec_changed(&self, cfg: &PluginConfig) -> bool {
        match self.launched.get(&cfg.name) {
            Some(was) => was.command != cfg.command || was.args != cfg.args || was.env != cfg.env,
            // Nothing to replace — it has to be started either way.
            None => false,
        }
    }
}
