//! The bookkeeping behind [`SizeOwnership`](super::SizeOwnership), and every
//! rule that reads or writes it.
//!
//! Split from the facade so that all of it happens where the fields live: the
//! rules are a handful of interlocking conditions over who is present, who owns
//! the sizing and how long it has been unattended, and spreading them across a
//! module boundary would mean opening those fields up to reach them.

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
    /// Present viewers in arrival order, newest last.
    arrival: Vec<ViewerId>,
    owner: Option<ViewerId>,
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

        // Arriving is not the only way in. An unowned sizing displaces nobody,
        // so the caution that stops a reconnect from taking a screen it never
        // claimed has nothing to protect — see the invariant in the module doc.
        let unowned = self.owner.is_none();
        let took = (arriving || unowned) && self.owner.as_ref() != Some(&viewer);
        if took {
            self.owner_absent_since = None;
            let displaced = self.owner.replace(viewer.clone());
            // Both hold for the first page of a session. Naming the arrival
            // there keeps the other reason meaning what it is worth reading:
            // a connection that took the sizing without anyone sitting down.
            audit::moved(
                displaced.as_ref(),
                Some(&viewer),
                if arriving {
                    "a viewer arrived"
                } else {
                    "nobody owned it"
                },
            );
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
        if self.owner.as_ref() == Some(&viewer) {
            return;
        }
        self.owner_absent_since = None;
        let displaced = self.owner.replace(viewer.clone());
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
        self.owner = self.arrival.last().cloned();
        audit::moved(gone.as_ref(), self.owner.as_ref(), "the owner stayed gone");
        if let Some(owner) = self.owner.clone() {
            self.tell(&owner, true);
        }
    }

    /// Tell every one of a viewer's connections whether it now owns the sizing.
    ///
    /// All of them, because a client keeps per-repository terminal state: an
    /// attached TUI holds one subscription per open repository and each has to
    /// re-fit its own panes. A connection whose queue is full is skipped rather
    /// than dropped — unwinding a client belongs to its hub, which will find it
    /// on the next frame it cannot deliver.
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
        let Ok(json) = serde_json::to_string(&ServerMessage::SizeOwner { owned }) else {
            return;
        };
        let _ = announce.tx.try_send(TerminalFrame::Control(json));
    }
}
