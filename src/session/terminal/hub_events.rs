//! Turning pane activity into plugin events, and plugin commands into judged
//! actions. Every function here is a no-op for a pane no plugin owns, so the
//! ordinary terminal path pays one hash lookup and nothing else.

use super::hub_plugins::{MAX_COMMANDS_PER_TICK, PANE_IDLE_THRESHOLD, Plugins};
use crate::backend::{PaneGeneration, PaneId, PaneToken, PtyBackend};
use crate::plugin::protocol::{PROTOCOL_VERSION, PluginCommand, PluginEvent};
use crate::plugin::{Approved, PaneFacts, Refused};
use crate::runtime::terminal::strip_escape_sequences;
use std::time::Instant;

impl Plugins {
    /// Send one pane-scoped event to whichever plugin owns `pane`, stamped with
    /// the pane's current identity. A dropped event is logged and nothing else —
    /// a plugin that cannot keep up must never stall the pane it is watching.
    fn send_for(
        &self,
        backend: &PtyBackend,
        pane: PaneId,
        build: impl FnOnce(PaneToken, PaneGeneration) -> PluginEvent,
    ) {
        let Some(name) = self.owners.get(&pane) else {
            return;
        };
        let Some(host) = self.hosts.get(name) else {
            return;
        };
        let Some(slot) = backend.slot(pane) else {
            return;
        };
        let event = build(slot.identity.token.clone(), slot.identity.generation);
        if !host.send(&event) {
            tracing::debug!(plugin = %name, pane, "viewer: plugin event dropped");
        }
    }

    /// Announce a pane a plugin has just been given, including one that a
    /// relaunch has just put back — the generation tells them apart.
    pub(super) fn pane_opened(&self, backend: &PtyBackend, pane: PaneId, title: Option<&str>) {
        let command = backend
            .slot(pane)
            .and_then(|slot| slot.launch.command.clone());
        let (title, cwd) = (title.map(str::to_string), self.cwd.clone());
        self.send_for(backend, pane, |token, generation| PluginEvent::PaneOpened {
            v: PROTOCOL_VERSION,
            token,
            generation,
            title,
            command,
            cwd,
        });
    }

    /// Feed a pane's output to its plugin as plain text. Stripped per chunk,
    /// which makes it best-effort: an escape sequence split across two reads
    /// survives in fragments. Acceptable because output text is only ever a
    /// fallback signal — nothing happens to a pane unless the plugin asks, and
    /// every ask goes through the guard.
    pub(super) fn pane_output(&mut self, backend: &PtyBackend, pane: PaneId, data: &[u8]) {
        if !self.owners.contains_key(&pane) {
            return;
        }
        // Fresh bytes end the quiet period, so the next one is announced again.
        self.idle_announced.remove(&pane);
        let text = strip_escape_sequences(data);
        if text.is_empty() {
            return;
        }
        self.send_for(backend, pane, |token, generation| PluginEvent::PaneOutput {
            v: PROTOCOL_VERSION,
            token,
            generation,
            text,
        });
    }

    /// The pane's process ended, but its slot is being held: a relaunch is
    /// possible until the hold expires.
    pub(super) fn pane_exited(&self, backend: &PtyBackend, pane: PaneId) {
        self.send_for(backend, pane, |token, generation| PluginEvent::PaneExited {
            v: PROTOCOL_VERSION,
            token,
            generation,
        });
    }

    /// The slot itself is going away, so nothing more can be done with it.
    /// Must be sent before the slot is retired — the event carries its identity.
    pub(super) fn pane_closed(&self, backend: &PtyBackend, pane: PaneId) {
        self.send_for(backend, pane, |token, generation| PluginEvent::PaneClosed {
            v: PROTOCOL_VERSION,
            token,
            generation,
        });
    }

    /// A human typed into the pane. Both halves matter: the plugin is told it
    /// has been taken over, and whatever it had already spent on this pane is
    /// dropped so its picture of the pane cannot outlive the person's.
    pub(super) fn user_input(&mut self, backend: &PtyBackend, pane: PaneId) {
        if !self.owners.contains_key(&pane) {
            return;
        }
        self.send_for(backend, pane, |token, generation| PluginEvent::UserInput {
            v: PROTOCOL_VERSION,
            token,
            generation,
        });
        if let Some(slot) = backend.slot(pane) {
            self.guard.cancel(&slot.identity.token.clone());
        }
    }

    /// Tell each plugin about any of its panes that has just gone quiet, once
    /// per quiet period.
    pub(super) fn notify_idle(&mut self, backend: &PtyBackend, now: Instant) {
        if self.owners.is_empty() {
            return;
        }
        let due: Vec<(PaneId, u64)> = self
            .owners
            .keys()
            .copied()
            .filter(|pane| !self.idle_announced.contains(pane))
            // An exited pane is quiet by definition; its plugin already had
            // `PaneExited`, which is the stronger signal.
            .filter(|pane| backend.is_process_alive(*pane))
            .filter_map(|pane| {
                let idle = backend.slot(pane)?.idle_for(now);
                (idle >= PANE_IDLE_THRESHOLD).then_some((pane, idle.as_millis() as u64))
            })
            .collect();
        for (pane, idle_ms) in due {
            self.send_for(backend, pane, |token, generation| PluginEvent::PaneIdle {
                v: PROTOCOL_VERSION,
                token,
                generation,
                idle_ms,
            });
            self.idle_announced.insert(pane);
        }
    }

    /// Take what every plugin has asked for since the last tick, at most
    /// [`MAX_COMMANDS_PER_TICK`] each so a chatty plugin cannot starve the panes.
    pub(super) fn take_commands(&self) -> Vec<(String, PluginCommand)> {
        let mut taken = Vec::new();
        for (name, host) in &self.hosts {
            for _ in 0..MAX_COMMANDS_PER_TICK {
                match host.try_recv() {
                    Some(command) => taken.push((name.clone(), command)),
                    None => break,
                }
            }
        }
        taken
    }

    /// Put one command through the guard.
    ///
    /// The pane the command names is resolved from its token — a plugin never
    /// sees a [`PaneId`] — and every fact the rules need is read from the
    /// backend here, so the guard itself holds no reference to it.
    pub(super) fn judge(
        &mut self,
        plugin: &str,
        command: PluginCommand,
        backend: &PtyBackend,
        now: Instant,
    ) -> Result<Approved, Refused> {
        let facts = command
            .token()
            .and_then(|token| backend.pane_for_token(token))
            .and_then(|pane| {
                let slot = backend.slot(pane)?;
                Some(PaneFacts {
                    pane,
                    generation: slot.identity.generation,
                    // This plugin's own claim on the pane, not any plugin's:
                    // one plugin must not be able to act through another's
                    // opt-in.
                    opted_in: self.owners.get(&pane).is_some_and(|name| name == plugin),
                    // The same lookup read the other way, which is a different
                    // question: an unclaimed pane may be asked for, one somebody
                    // else claimed may not.
                    watched_by_another: self.owners.get(&pane).is_some_and(|name| name != plugin),
                    may_watch_on_signal: self.watch_on_signal.contains(plugin),
                    alive: backend.is_process_alive(pane),
                    idle: slot.idle_for(now),
                    launch_command: slot.launch.command.clone(),
                })
            });
        let allowed = self
            .allowed_flags
            .get(plugin)
            .map(Vec::as_slice)
            .unwrap_or_default();
        self.guard.judge(command, facts.as_ref(), allowed, now)
    }
}
