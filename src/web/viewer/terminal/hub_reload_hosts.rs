//! Starting and stopping one plugin's child during a reload.
//!
//! Split from the diff that decides *which* children should run, because these
//! carry the awkward part: what happens to the panes a plugin was watching, which
//! differs by whether a replacement is about to take its place.

use super::hub_plugins::Plugins;
use super::hub_reload::PluginReload;
use crate::backend::{PaneId, PtyBackend};
use crate::config::PluginConfig;
use std::collections::HashMap;

impl Plugins {
    /// Launch `cfg`'s child and hand it the panes it owns. Reports whether the
    /// child came up — a caller that stopped a predecessor for this one has to
    /// know it never arrived.
    pub(super) fn start_host(
        &mut self,
        cfg: &PluginConfig,
        backend: &PtyBackend,
        titles: &HashMap<PaneId, Option<String>>,
        outcome: &mut PluginReload,
    ) -> bool {
        let dir = crate::plugin::registry::default_plugins_dir().ok();
        match crate::plugin::PluginHost::spawn(cfg, dir.as_deref()) {
            Ok(host) => {
                self.allowed_flags
                    .insert(cfg.name.clone(), cfg.allowed_resume_flags.clone());
                self.set_watch_on_signal(&cfg.name, cfg.watch_on_signal);
                self.launched.insert(cfg.name.clone(), cfg.clone());
                self.hosts.insert(cfg.name.clone(), host);
                // Every pane that opted into this plugin is handed to it, whether
                // it was already owned — the child that knew about it is gone — or
                // was never adopted because there was no host when it opened. The
                // second case is what makes enabling a plugin mid-session useful:
                // a pane created while it was off is still the pane its own
                // configuration named.
                //
                // Only panes the hub still has. `titles` is that set, so a pane
                // that has since exited is skipped rather than announced to a
                // plugin that could do nothing about it.
                let opted_in: Vec<PaneId> = self
                    .intended
                    .iter()
                    .filter(|(pane, named)| *named == &cfg.name && titles.contains_key(pane))
                    .map(|(pane, _)| *pane)
                    .collect();
                for pane in opted_in {
                    self.owners.insert(pane, cfg.name.clone());
                    let title = titles.get(&pane).and_then(Option::as_deref);
                    self.pane_opened(backend, pane, title);
                }
                outcome.started.push(cfg.name.clone());
                true
            }
            // Left out exactly as at startup: its panes behave like unwatched
            // ones, so a broken plugin costs a warning rather than a terminal.
            Err(error) => {
                tracing::warn!(
                    plugin = %cfg.name,
                    %error,
                    "viewer: plugin did not relaunch on reload; its panes run unwatched"
                );
                false
            }
        }
    }

    /// Stop watching `pane` without forgetting what it opted into.
    ///
    /// The narrower half of [`Plugins::forget`], for a pane that is still there:
    /// the association, any relaunch hold and the spent budget go, so the pane
    /// becomes an ordinary terminal, but the opt-in stays. That is what makes
    /// disabling a plugin and enabling it again land where enabling it the first
    /// time would — the alternative is a switch that means something different
    /// depending on which way it was last flipped.
    fn release_pane(&mut self, backend: &PtyBackend, pane: PaneId) {
        self.owners.remove(&pane);
        self.pending.remove(&pane);
        self.idle_announced.remove(&pane);
        if let Some(slot) = backend.slot(pane) {
            self.guard.cancel(&slot.identity.token.clone());
        }
    }

    /// Stop `name`'s child and, unless a replacement is about to take its place,
    /// release every pane it was watching.
    pub(super) fn stop_host(
        &mut self,
        backend: &mut PtyBackend,
        name: &str,
        replaced: bool,
        outcome: &mut PluginReload,
    ) {
        if let Some(mut host) = self.hosts.remove(name) {
            host.shutdown();
        }
        self.launched.remove(name);
        // Every hold this plugin had goes, replacement or not. A hold is a pane
        // whose process already exited, kept alive only so *that* plugin could
        // relaunch it — and the successor is never told about it. It is handed
        // back the panes the hub still has (see `start_host`), and an exited one
        // is not among them, so its token dies with the child that was given it.
        // Left in place the slot would sit out its whole window with nothing that
        // could honour it, while every client counted down to a relaunch that was
        // never coming.
        self.retire_holds_of(backend, name, outcome);
        if replaced {
            // The live panes stay this plugin's, and are handed to the successor
            // once it is up. The guard state stays with them: it is what bounds
            // the relaunches the successor may ask for, and a fresh allowance on
            // every reload would mean the ceiling was never reached.
            outcome.restarted.push(name.to_string());
            return;
        }
        self.abandon(backend, name, outcome);
    }

    /// Let go of everything `name` still holds: its rules, and the panes it was
    /// watching.
    ///
    /// Reached two ways — a plugin the file dropped, and a replacement whose
    /// child never came up. The second is why this is separate: [`stop_host`]
    /// keeps the live panes when a successor is on the way, and if the spawn then
    /// fails those panes are owned by a name with no host. Left that way, the
    /// pane's next exit takes the relaunch path — its slot held open for a plugin
    /// that cannot ask — and on a hub whose only plugin this was, nothing even
    /// runs to expire the hold, so the client counts down to a deadline that
    /// never arrives.
    ///
    /// [`stop_host`]: Self::stop_host
    pub(super) fn abandon(&mut self, backend: &PtyBackend, name: &str, outcome: &mut PluginReload) {
        self.allowed_flags.remove(name);
        self.set_watch_on_signal(name, false);
        let owned: Vec<PaneId> = self.panes_of(name);
        for pane in owned {
            self.release_pane(backend, pane);
        }
        // Whatever `stop_host` recorded as a restart was not one.
        outcome.restarted.retain(|plugin| plugin != name);
        outcome.stopped.push(name.to_string());
    }

    /// Retire every slot `name` was holding for a relaunch, reporting the panes so
    /// the caller can tell the clients showing their deadlines.
    fn retire_holds_of(
        &mut self,
        backend: &mut PtyBackend,
        name: &str,
        outcome: &mut PluginReload,
    ) {
        for pane in self.panes_of(name) {
            if self.claim_pending(pane).is_none() {
                continue;
            }
            // `release_pane` before `retire_slot`: the spent budget is keyed by
            // the slot's token, so it has to be read while the slot is still there.
            self.release_pane(backend, pane);
            backend.retire_slot(pane);
            outcome.retired.push(pane);
        }
    }

    /// The panes `name` currently owns.
    fn panes_of(&self, name: &str) -> Vec<PaneId> {
        self.owners
            .iter()
            .filter(|(_, owner)| *owner == name)
            .map(|(pane, _)| *pane)
            .collect()
    }
}
