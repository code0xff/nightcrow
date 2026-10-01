//! The bookkeeping behind [`SizeOwnership`](super::SizeOwnership), and every
//! rule that reads or writes it. Split from the facade so the rules — a handful
//! of interlocking conditions over presence, ownership and idle time — live
//! where the fields are, rather than opening those fields across a module
//! boundary.

use super::{RELEASE_GRACE, Registration, ViewerId, audit};
use crate::session::terminal::frame::{ServerMessage, TerminalFrame};
use std::collections::HashMap;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

/// A registered connection: which viewer it belongs to, and how to tell it.
struct Announce {
    viewer: ViewerId,
    tx: SyncSender<TerminalFrame>,
}

#[derive(Default)]
pub(super) struct Inner {
    /// How many connections each present viewer holds.
    present: HashMap<ViewerId, usize>,
    /// Present viewers in first-connection order, newest last.
    arrival: Vec<ViewerId>,
    /// Viewers that have taken control, ordered by their last user activity.
    activity: Vec<ViewerId>,
    owner: Option<ViewerId>,
    /// Invalidates a queued resize after any ownership change, including A → B → A.
    generation: u64,
    /// When the owner's last connection went, if it has none now.
    owner_absent_since: Option<Instant>,
    /// Every live connection, by the id the registrar handed out.
    announce: HashMap<u64, Announce>,
    next_connection: u64,
}

impl Inner {
    pub(super) fn join(
        &mut self,
        viewer: ViewerId,
        arriving: bool,
        tx: SyncSender<TerminalFrame>,
        now: Instant,
    ) -> Registration {
        self.expire_absent_owner(now);

        let connection = self.next_connection;
        self.next_connection += 1;
        let count = self.present.entry(viewer.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            self.arrival.push(viewer.clone());
        }
        if self.owner.as_ref() == Some(&viewer) {
            // The owner is back, or was never really gone.
            self.owner_absent_since = None;
        }
        self.announce.insert(
            connection,
            Announce {
                viewer: viewer.clone(),
                tx,
            },
        );

        audit::joined(&viewer, connection, arriving);

        // A new viewer only bootstraps a session with no sizing owner.
        let unowned = self.owner.is_none();
        let took = unowned && self.owner.as_ref() != Some(&viewer);
        if took {
            self.owner_absent_since = None;
            let displaced = self.owner.replace(viewer.clone());
            self.mark_active(&viewer);
            self.advance_generation();
            audit::moved(displaced.as_ref(), Some(&viewer), "nobody owned it");
            // Every one of the new owner's connections, and of the displaced
            // one's: each holds its own repository's panes to re-fit or stop
            // sizing.
            self.tell(&viewer, true);
            if let Some(displaced) = displaced {
                self.tell(&displaced, false);
            }
        } else {
            // Nothing moved. Only the connection that just opened needs telling,
            // because only it does not know yet.
            self.tell_one(connection, self.owner.as_ref() == Some(&viewer));
        }
        #[cfg(test)]
        let owned = self.owner.as_ref() == Some(&viewer);
        Registration {
            connection,
            #[cfg(test)]
            owned,
        }
    }

    pub(super) fn leave(&mut self, connection: u64, now: Instant) {
        let Some(gone) = self.announce.remove(&connection) else {
            return;
        };
        let still_present = match self.present.get_mut(&gone.viewer) {
            Some(count) => {
                *count -= 1;
                *count > 0
            }
            None => false,
        };
        audit::left(&gone.viewer, connection, !still_present);
        if still_present {
            return;
        }
        self.present.remove(&gone.viewer);
        self.arrival.retain(|v| v != &gone.viewer);
        self.activity.retain(|v| v != &gone.viewer);
        if self.owner.as_ref() == Some(&gone.viewer) {
            self.owner_absent_since = Some(now);
        }
    }

    pub(super) fn claim(&mut self, connection: u64, now: Instant) {
        self.expire_absent_owner(now);
        let Some(viewer) = self.announce.get(&connection).map(|a| a.viewer.clone()) else {
            // A claim can arrive after its connection went; there is nobody left
            // to hand the sizing to.
            return;
        };
        self.mark_active(&viewer);
        if self.owner.as_ref() == Some(&viewer) {
            self.tell_one(connection, true);
            return;
        }
        self.owner_absent_since = None;
        let displaced = self.owner.replace(viewer.clone());
        self.advance_generation();
        audit::moved(displaced.as_ref(), Some(&viewer), "a viewer asked");
        self.tell(&viewer, true);
        if let Some(displaced) = displaced {
            self.tell(&displaced, false);
        }
    }

    pub(super) fn owns(&self, connection: u64) -> bool {
        self.announce
            .get(&connection)
            .is_some_and(|a| self.owner.as_ref() == Some(&a.viewer))
    }

    pub(super) fn owner_generation(&self, connection: u64) -> Option<u64> {
        self.owns(connection).then_some(self.generation)
    }

    pub(super) fn with_owner_generation<R>(
        &self,
        connection: u64,
        generation: u64,
        apply: impl FnOnce() -> R,
    ) -> Option<R> {
        (self.owner_generation(connection) == Some(generation)).then(apply)
    }

    fn mark_active(&mut self, viewer: &ViewerId) {
        self.activity.retain(|active| active != viewer);
        self.activity.push(viewer.clone());
    }

    fn advance_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    #[cfg(test)]
    pub(super) fn owner(&self) -> Option<ViewerId> {
        self.owner.clone()
    }

    /// Give the sizing to the most recent viewer still here, once the owner has
    /// been without a connection for longer than the grace.
    pub(super) fn expire_absent_owner(&mut self, now: Instant) {
        let Some(since) = self.owner_absent_since else {
            return;
        };
        if now.duration_since(since) < RELEASE_GRACE {
            return;
        }
        self.owner_absent_since = None;
        // The same rule that gave it away in the first place. With nobody left
        // it goes unowned and every pane keeps the size it has — there is no
        // client to fit, and the next viewer to connect picks it up.
        let gone = self.owner.take();
        self.owner = self
            .activity
            .last()
            .or_else(|| self.arrival.last())
            .cloned();
        if gone != self.owner {
            self.advance_generation();
        }
        audit::moved(gone.as_ref(), self.owner.as_ref(), "the owner stayed gone");
        if let Some(owner) = self.owner.clone() {
            self.tell(&owner, true);
        }
    }

    /// Tell every one of a viewer's connections whether it now owns the sizing.
    ///
    /// All of them, because a client keeps per-repository terminal state: an
    /// attached TUI holds one subscription per open repository and each has to
    /// re-fit its own panes. A full bounded queue drops this verdict without
    /// blocking; the next claim or reconnect announces the current state again.
    fn tell(&self, viewer: &ViewerId, owned: bool) {
        for (connection, announce) in &self.announce {
            if &announce.viewer == viewer {
                self.tell_one(*connection, owned);
            }
        }
    }

    /// Tell one connection where the sizing stands. For a connection that has
    /// just opened: nothing moved, but it does not know that yet.
    fn tell_one(&self, connection: u64, owned: bool) {
        let Some(announce) = self.announce.get(&connection) else {
            return;
        };
        let Ok(json) = serde_json::to_string(&ServerMessage::SizeOwner {
            owned,
            generation: self.generation.to_string(),
        }) else {
            return;
        };
        let _ = announce.tx.try_send(TerminalFrame::Control(json));
    }
}
