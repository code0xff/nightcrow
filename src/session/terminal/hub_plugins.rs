//! The plugin side of a terminal worker: which panes a plugin may see, the
//! hosts watching them, and the slots being held open for a relaunch.
//!
//! Every field here is worker-local. A plugin can drive a pane's keyboard, so
//! none of this is reachable from a connection thread — the only way in is the
//! command queue the worker already drains, and the only way out is
//! [`crate::plugin::Guard`].
//!
//! A pane appears here only two ways: its `[[startup_command]]` named a plugin
//! by hand, or a plugin asked for it by quoting the pane's own token and the
//! guard allowed it. Neither can be reached by a plugin enumerating panes,
//! because nothing ever tells a plugin what panes exist — which is what keeps
//! "an arbitrary shell is never plugin-controlled" a property of the code.

use crate::backend::PaneId;
use crate::config::{PluginConfig, StartupCommand};
use crate::plugin::{Guard, PluginHost, RateLimits};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// How long a pane must be quiet before its plugin is told it is idle.
/// Deliberately the same value the guard requires before it will let a plugin
/// type into that pane. Ten seconds is well past the pause a CLI takes
/// mid-answer and well short of a wait a person would notice.
pub(super) const PANE_IDLE_THRESHOLD: Duration = Duration::from_secs(10);

/// Commands taken from any one plugin per loop iteration. One thread serves
/// every pane in the repository, so a plugin that writes without pause must
/// not be able to hold it. Eight per 8 ms tick is a thousand a second — far
/// past anything a legitimate plugin needs, and bounded.
pub(super) const MAX_COMMANDS_PER_TICK: usize = 8;

pub(super) struct Plugins {
    /// Live plugin children, by configured name.
    pub(super) hosts: HashMap<String, PluginHost>,
    /// What each live host was launched from, so a config reload can tell a
    /// plugin whose process must be replaced from one whose rules merely changed.
    pub(super) launched: HashMap<String, PluginConfig>,
    /// Which plugin owns which pane. The authority for `opted_in`.
    pub(super) owners: HashMap<PaneId, String>,
    /// Which plugin a pane's *configuration* named, recorded whether or not that
    /// plugin had a host at the time. Separate from `owners` because it grants
    /// nothing: a pane is only ever acted on through `owners`. What it is for is
    /// a config reload — enabling a plugin whose panes were created while it was
    /// off has to be able to find them.
    pub(super) intended: HashMap<PaneId, String>,
    /// Where this hub's plugins listen. Held so a config reload starts a host
    /// on the same socket directory the running panes were told about.
    pub(super) runtime_dir: Option<std::path::PathBuf>,
    /// Each plugin's `allowed_resume_flags`, as the guard needs them.
    pub(super) allowed_flags: HashMap<String, Vec<String>>,
    /// The plugins whose config set `watch_on_signal`: those the operator allowed
    /// to be given a pane they were never named by.
    pub(super) watch_on_signal: HashSet<String>,
    pub(super) guard: Guard,
    pub(super) pending: HashMap<PaneId, super::hub_plugins_slots::Pending>,
    /// Panes whose plugin has already been told about the current quiet period,
    /// so it is told once rather than on every tick.
    pub(super) idle_announced: HashSet<PaneId>,
    /// The repository the panes run in, reported with every `PaneOpened`.
    pub(super) cwd: String,
}

impl Plugins {
    /// Launch a host for every plugin that is enabled *and* has some pane it
    /// could be given.
    ///
    /// Both conditions, because a host with no pane to watch is a child process
    /// that can never be given anything to do. `watch_on_signal` satisfies the
    /// second: such a plugin has to be running *before* any of its panes speak,
    /// or waiting for an opt-in that never comes makes the switch mean nothing.
    /// A plugin that will not launch is logged and left out: its panes then
    /// behave exactly like unwatched ones, so a broken plugin costs the
    /// operator a warning rather than a terminal.
    pub(super) fn start(cwd: &str, configs: &[PluginConfig], startup: &[StartupCommand]) -> Self {
        let dir = crate::plugin::registry::default_plugins_dir()
            .inspect_err(|error| {
                tracing::debug!(%error, "viewer: no plugin directory; resolving plugins on PATH");
            })
            .ok();
        // Derived from the hub's own path, exactly as the pane spawn derives
        // it, so this hub's plugins and this hub's panes agree on where the
        // sockets are without one telling the other.
        let runtime_dir = crate::backend::identity::plugin_runtime_dir(std::path::Path::new(cwd));
        let mut hosts = HashMap::new();
        let mut launched = HashMap::new();
        let mut allowed_flags = HashMap::new();
        let mut watch_on_signal = HashSet::new();
        for cfg in configs {
            let opted_in = startup
                .iter()
                .any(|sc| sc.plugin.as_deref() == Some(cfg.name.as_str()));
            if !cfg.enabled || !(opted_in || cfg.watch_on_signal) {
                continue;
            }
            match PluginHost::spawn(cfg, dir.as_deref(), runtime_dir.as_deref()) {
                Ok(host) => {
                    allowed_flags.insert(cfg.name.clone(), cfg.allowed_resume_flags.clone());
                    if cfg.watch_on_signal {
                        watch_on_signal.insert(cfg.name.clone());
                    }
                    launched.insert(cfg.name.clone(), cfg.clone());
                    hosts.insert(cfg.name.clone(), host);
                }
                Err(error) => tracing::warn!(
                    plugin = %cfg.name,
                    %error,
                    "viewer: plugin did not launch; its panes run unwatched"
                ),
            }
        }
        Self {
            hosts,
            launched,
            runtime_dir,
            owners: HashMap::new(),
            intended: HashMap::new(),
            allowed_flags,
            watch_on_signal,
            guard: Guard::new(PANE_IDLE_THRESHOLD, RateLimits::default()),
            pending: HashMap::new(),
            idle_announced: HashSet::new(),
            cwd: cwd.to_string(),
        }
    }

    /// Hand `pane` to `plugin`, reporting whether it took.
    ///
    /// Refused when that plugin has no host: recording an association nothing
    /// can act on would put the pane on the relaunch path for no benefit.
    pub(super) fn adopt(&mut self, pane: PaneId, plugin: &str) -> bool {
        // Recorded either way: what the pane asked for is a fact about the pane,
        // and a reload that later enables this plugin has no other way to learn
        // it. It grants nothing on its own — see `intended`.
        self.intended.insert(pane, plugin.to_string());
        if !self.hosts.contains_key(plugin) {
            return false;
        }
        self.owners.insert(pane, plugin.to_string());
        true
    }

    /// Which plugin watches `pane`, if any.
    pub(super) fn owner(&self, pane: PaneId) -> Option<&str> {
        self.owners.get(&pane).map(String::as_str)
    }

    /// Turn `name`'s `watch_on_signal` permission on or off, as a config reload
    /// may. Held as a set membership, so this is the whole of the switch.
    pub(super) fn set_watch_on_signal(&mut self, name: &str, allowed: bool) {
        if allowed {
            self.watch_on_signal.insert(name.to_string());
        } else {
            self.watch_on_signal.remove(name);
        }
    }

    /// Whether there is nothing for the per-tick work to do, which is the common
    /// case: no host means no pane can be watched and no command can arrive.
    ///
    /// Deliberately *not* "nothing is watched yet". A host's inbound queue is
    /// drained only by that per-tick work, so skipping it while a live plugin was
    /// writing — before any startup pane had been claimed, say — would let its
    /// commands pile up unread with nothing to bound them.
    pub(super) fn is_inert(&self) -> bool {
        self.hosts.is_empty()
    }

    /// Stop every plugin child.
    ///
    /// The only place that happens: a plugin is not one of `PtyBackend`'s panes,
    /// so nothing else will ever reap it.
    pub(super) fn shutdown(&mut self) {
        for (name, host) in self.hosts.iter_mut() {
            let dropped = host.dropped_events();
            if dropped > 0 {
                tracing::warn!(plugin = %name, dropped, "viewer: plugin fell behind its events");
            }
            host.shutdown();
        }
    }
}
