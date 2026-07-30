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

use super::hub_helpers::PaneState;
use crate::backend::{PaneId, PtyBackend};
use crate::config::{PluginConfig, StartupCommand};
use crate::plugin::{Guard, PluginHost, RateLimits};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// How long a pane must be quiet before its plugin is told it is idle.
///
/// Deliberately the same value the guard requires before it will let a plugin
/// type into that pane: announcing idleness any earlier would only invite
/// commands the guard is bound to refuse. Ten seconds is well past the pause a
/// CLI takes mid-answer and well short of a wait a person would notice.
pub(super) const PANE_IDLE_THRESHOLD: Duration = Duration::from_secs(10);

/// How long an exited pane's slot is kept so a relaunch can still reuse its
/// token.
///
/// This is a backstop against a plugin that died or lost interest, so it has to
/// outlast every wait a plugin may legitimately be in the middle of. Providers
/// quote windows in hours *and* in days — a weekly quota is a real case — so a
/// value picked around the five-hour window would silently throw the pane's
/// identity away days before the wait paid off, and the relaunch it was being
/// kept for would fail. Nine days clears the longest window a bundled plugin
/// will wait out (`nightcrow-recovery`'s own clamp is eight days) with slack for
/// a reset that lands late.
///
/// Holding it that long is cheap on purpose: a token, a generation and a command
/// string. The process, its fds and its threads were let go the moment it exited
/// (see [`PtyBackend::release_process`]), and closing the pane or stopping the
/// session retires the slot immediately either way.
pub(super) const PENDING_RELAUNCH_TTL: Duration = Duration::from_secs(9 * 24 * 60 * 60);

/// Commands taken from any one plugin per loop iteration.
///
/// One thread serves every pane in the repository, so a plugin that writes
/// without pause must not be able to hold it. Eight per 8 ms tick is a thousand
/// a second — far past anything a legitimate plugin needs, and bounded.
pub(super) const MAX_COMMANDS_PER_TICK: usize = 8;

/// Where a pane sat and what it looked like, captured before it is removed.
pub(super) struct PaneSpot {
    /// Its position in the client-visible order, so a relaunch lands back where
    /// the operator left it instead of at the end of the row.
    pub(super) index: usize,
    pub(super) rows: u16,
    pub(super) cols: u16,
    pub(super) title: Option<String>,
}

impl PaneSpot {
    pub(super) fn of(index: usize, pane: &PaneState) -> Self {
        Self {
            index,
            rows: pane.rows,
            cols: pane.cols,
            title: pane.title.clone(),
        }
    }
}

/// A pane whose process exited while a plugin was watching it, held so that
/// plugin still has something to relaunch.
pub(super) struct Pending {
    pub(super) spot: PaneSpot,
    /// When the slot is given up on. See [`PENDING_RELAUNCH_TTL`].
    deadline: Instant,
}

pub(super) struct Plugins {
    /// Live plugin children, by configured name. A plugin that failed to launch
    /// is absent, and its panes are therefore never adopted below.
    pub(super) hosts: HashMap<String, PluginHost>,
    /// Which plugin owns which pane. The authority for `opted_in`.
    pub(super) owners: HashMap<PaneId, String>,
    /// Each plugin's `allowed_resume_flags`, as the guard needs them.
    pub(super) allowed_flags: HashMap<String, Vec<String>>,
    /// The plugins whose config set `watch_on_signal`: those the operator allowed
    /// to be given a pane they were never named by. Held as the set of names
    /// rather than looked up in the config list, so the judgement reads it as a
    /// hash probe on the same footing as every other fact it gathers.
    pub(super) watch_on_signal: HashSet<String>,
    pub(super) guard: Guard,
    pub(super) pending: HashMap<PaneId, Pending>,
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
            owners: HashMap::new(),
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

    /// Hold `pane`'s slot open for a relaunch. Nothing is held for a pane no
    /// plugin watches — that pane's slot is gone by the time this is reached.
    pub(super) fn hold_for_relaunch(&mut self, pane: PaneId, spot: PaneSpot, now: Instant) {
        if !self.owners.contains_key(&pane) {
            return;
        }
        self.idle_announced.remove(&pane);
        self.pending.insert(
            pane,
            Pending {
                spot,
                deadline: now + PENDING_RELAUNCH_TTL,
            },
        );
    }

    /// Take the hold on `pane`, if it is still within its window.
    pub(super) fn claim_pending(&mut self, pane: PaneId) -> Option<Pending> {
        self.pending.remove(&pane)
    }

    /// Put a hold back after a relaunch attempt failed, so the pane keeps its
    /// remaining window instead of being retired by a single bad try.
    pub(super) fn restore_pending(&mut self, pane: PaneId, pending: Pending) {
        self.pending.insert(pane, pending);
    }

    /// Move a pane's association onto the process that replaced it.
    ///
    /// The spent budget is deliberately left alone. It is keyed by the slot's
    /// token, which a relaunch preserves, and that is the only thing bounding a
    /// plugin that answers every exit with another relaunch — clearing it here
    /// would hand out a fresh allowance on every attempt and the ceiling would
    /// never be reached.
    pub(super) fn take_over(&mut self, old: PaneId, new: PaneId) {
        if let Some(plugin) = self.owners.remove(&old) {
            self.owners.insert(new, plugin);
        }
        self.idle_announced.remove(&old);
    }

    /// Forget `pane` entirely. The caller still has to retire its slot.
    ///
    /// Takes the backend because the budget is keyed by the slot's token, so it
    /// has to be read before the slot is retired.
    pub(super) fn forget(&mut self, backend: &PtyBackend, pane: PaneId) {
        self.owners.remove(&pane);
        self.pending.remove(&pane);
        self.idle_announced.remove(&pane);
        if let Some(slot) = backend.slot(pane) {
            self.guard.cancel(&slot.identity.token.clone());
        }
    }

    /// Retire the slots nobody relaunched in time, reporting which panes those
    /// were so the caller can tell the clients still showing their deadlines.
    pub(super) fn expire_pending(&mut self, backend: &mut PtyBackend, now: Instant) -> Vec<PaneId> {
        let expired: Vec<PaneId> = self
            .pending
            .iter()
            .filter(|(_, held)| now >= held.deadline)
            .map(|(pane, _)| *pane)
            .collect();
        for pane in &expired {
            tracing::info!(
                pane,
                "viewer: no relaunch within the window; retiring the pane's slot"
            );
            self.forget(backend, *pane);
            backend.retire_slot(*pane);
        }
        expired
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
