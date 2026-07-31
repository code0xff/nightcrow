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
//! guard allowed it (see `plugin::guard_watch`). Neither can be reached
//! by a plugin enumerating panes, because nothing ever tells a plugin what panes
//! exist — which is what keeps "an arbitrary shell is never plugin-controlled" a
//! property of the code. A shell stays untouched by doing what a shell does:
//! saying nothing to any plugin.

use crate::backend::PaneId;
use crate::config::{PluginConfig, StartupCommand};
use crate::plugin::{Guard, PluginHost, RateLimits};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// How long a pane must be quiet before its plugin is told it is idle.
///
/// Deliberately the same value the guard requires before it will let a plugin
/// type into that pane: announcing idleness any earlier would only invite
/// commands the guard is bound to refuse. Ten seconds is well past the pause a
/// CLI takes mid-answer and well short of a wait a person would notice.
pub(super) const PANE_IDLE_THRESHOLD: Duration = Duration::from_secs(10);

/// Commands taken from any one plugin per loop iteration.
///
/// One thread serves every pane in the repository, so a plugin that writes
/// without pause must not be able to hold it. Eight per 8 ms tick is a thousand
/// a second — far past anything a legitimate plugin needs, and bounded.
pub(super) const MAX_COMMANDS_PER_TICK: usize = 8;

pub(super) struct Plugins {
    /// Live plugin children, by configured name. A plugin that failed to launch
    /// is absent, and its panes are therefore never adopted below.
    pub(super) hosts: HashMap<String, PluginHost>,
    /// What each live host was launched from, so a config reload can tell a
    /// plugin whose process must be replaced from one whose rules merely
    /// changed. Kept beside the hosts rather than read back off the child, which
    /// knows only its pipes.
    pub(super) launched: HashMap<String, PluginConfig>,
    /// Which plugin owns which pane. The authority for `opted_in`.
    pub(super) owners: HashMap<PaneId, String>,
    /// Which plugin a pane's *configuration* named, recorded whether or not that
    /// plugin had a host at the time.
    ///
    /// Separate from `owners` because it grants nothing: a pane is only ever
    /// acted on through `owners`, so an entry here with no host behind it puts
    /// the pane on no relaunch path and sends no events — the reason `adopt`
    /// refuses such an association in the first place. What it is for is a config
    /// reload: enabling a plugin whose panes were created while it was off has to
    /// be able to find them, and the opt-in is the only record that they were
    /// ever meant for it. A pane closing takes its entry with it, so this is
    /// bounded by the live panes and never by the session's history.
    pub(super) intended: HashMap<PaneId, String>,
    /// Each plugin's `allowed_resume_flags`, as the guard needs them.
    pub(super) allowed_flags: HashMap<String, Vec<String>>,
    /// The plugins whose config set `watch_on_signal`: those the operator allowed
    /// to be given a pane they were never named by. Held as the set of names
    /// rather than looked up in the config list, so the judgement reads it as a
    /// hash probe on the same footing as every other fact it gathers.
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
    /// that can never be given anything to do. `watch_on_signal` is the second
    /// way to satisfy the first: such a plugin's panes are the ones that will
    /// speak to it, so it has to be running *before* any of them does — waiting
    /// for an opt-in that will never come would make the switch mean nothing.
    /// A plugin that will not launch is logged and left out: its panes then
    /// behave exactly like unwatched ones, so a broken plugin costs the operator
    /// a warning rather than a terminal.
    pub(super) fn start(cwd: &str, configs: &[PluginConfig], startup: &[StartupCommand]) -> Self {
        let dir = crate::plugin::registry::default_plugins_dir()
            .inspect_err(|error| {
                tracing::debug!(%error, "viewer: no plugin directory; resolving plugins on PATH");
            })
            .ok();
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
            match PluginHost::spawn(cfg, dir.as_deref()) {
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
    /// can act on would put the pane on the relaunch path — its slot kept alive
    /// after an exit for a plugin that will never ask — for no benefit.
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
