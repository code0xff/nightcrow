//! Which screen the session's PTYs are fitted to.
//!
//! A PTY is a contract with a child process: the child draws for the width it
//! was told, and nothing can re-flow an alternate-screen program afterwards. So
//! the size is one value with one owner — the most recent viewer to arrive
//! (tmux's `window-size latest`), until another takes it.
//!
//! **Why this is the session's and not each hub's.** Which repository is in
//! front is shared by the whole session, so "which screen is this session fitted
//! to" is one question, not one per repository. Asked per hub, it was re-answered
//! from scratch on every switch — a browser's terminal socket is tied to the
//! repository it shows, so moving tabs made every attached page reconnect at once
//! and the sizing fell to whichever handshake finished last.
//!
//! **A viewer is not a connection.** A socket opens for reasons that are not a
//! person sitting down: a repository switch, a page reload, a network blip. So a
//! viewer names itself ([`ViewerId`]) and says outright whether it is newly
//! arrived; the session never infers it. Connections come and go beneath a
//! viewer without moving anything.

use crate::web::viewer::terminal::frame::{ServerMessage, TerminalFrame};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc::SyncSender;
use std::time::{Duration, Instant};

/// How long the sizing is held for an owner that has no connection left.
///
/// Switching repositories closes one terminal socket and opens another, and for
/// the moment in between the owner is not connected to anything. Handing the
/// sizing away there and back again would re-fit every pane twice for a viewer
/// that never went anywhere. Only the *release* is delayed; nothing claims by
/// waiting.
pub const RELEASE_GRACE: Duration = Duration::from_secs(2);

/// Who a client is, across however many connections it holds.
///
/// Two variants so a browser cannot name itself an attached terminal: the
/// browser half is client-supplied, the other is minted by the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ViewerId {
    /// One browser tab, by the id it generated for itself.
    Browser(String),
    /// One attached TUI, by its daemon client id.
    Attached(u64),
}

/// A registered connection: which viewer it belongs to, and how to tell it.
struct Announce {
    viewer: ViewerId,
    tx: SyncSender<TerminalFrame>,
}

#[derive(Default)]
struct Inner {
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

/// The session's size ownership. Shared by every terminal hub.
#[derive(Default)]
pub struct SizeOwnership {
    inner: Mutex<Inner>,
}

/// A connection's registration. Dropping it is not enough — the holder calls
/// [`SizeOwnership::leave`].
pub struct Registration {
    /// The key to unregister with.
    pub connection: u64,
    /// Whether this connection's viewer owns the sizing right now.
    pub owned: bool,
}

impl SizeOwnership {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a connection for `viewer`.
    ///
    /// `arriving` is the client's own word for "a person just sat down here" —
    /// a page opening rather than a repository switch or a reconnect. Only that
    /// takes the sizing.
    pub fn join(
        &self,
        viewer: ViewerId,
        arriving: bool,
        tx: SyncSender<TerminalFrame>,
        now: Instant,
    ) -> Registration {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.expire_absent_owner(now);

        let connection = inner.next_connection;
        inner.next_connection += 1;
        let count = inner.present.entry(viewer.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            inner.arrival.push(viewer.clone());
        }
        if inner.owner.as_ref() == Some(&viewer) {
            // The owner is back, or was never really gone.
            inner.owner_absent_since = None;
        }
        inner.announce.insert(
            connection,
            Announce {
                viewer: viewer.clone(),
                tx,
            },
        );

        let took = arriving && inner.owner.as_ref() != Some(&viewer);
        if took {
            inner.owner_absent_since = None;
            let displaced = inner.owner.replace(viewer.clone());
            // Every one of the new owner's connections, and of the displaced
            // one's: each holds its own repository's panes to re-fit or stop
            // sizing.
            inner.tell(&viewer, true);
            if let Some(displaced) = displaced {
                inner.tell(&displaced, false);
            }
        } else {
            // Nothing moved. Only the connection that just opened needs telling,
            // because only it does not know yet.
            inner.tell_one(connection, inner.owner.as_ref() == Some(&viewer));
        }
        let owned = inner.owner.as_ref() == Some(&viewer);
        Registration { connection, owned }
    }

    /// Drop a connection. The sizing moves only when its viewer has no other,
    /// and even then not at once — see [`RELEASE_GRACE`].
    pub fn leave(&self, connection: u64, now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let Some(gone) = inner.announce.remove(&connection) else {
            return;
        };
        let still_present = match inner.present.get_mut(&gone.viewer) {
            Some(count) => {
                *count -= 1;
                *count > 0
            }
            None => false,
        };
        if still_present {
            return;
        }
        inner.present.remove(&gone.viewer);
        inner.arrival.retain(|v| v != &gone.viewer);
        if inner.owner.as_ref() == Some(&gone.viewer) {
            inner.owner_absent_since = Some(now);
        }
    }

    /// Take the sizing at a viewer's own request — the fit button, or the TUI's
    /// chord. Unlike arriving, this is unconditional.
    pub fn claim(&self, connection: u64, now: Instant) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.expire_absent_owner(now);
        let Some(viewer) = inner.announce.get(&connection).map(|a| a.viewer.clone()) else {
            // A claim can arrive after its connection went; there is nobody left
            // to hand the sizing to.
            return;
        };
        if inner.owner.as_ref() == Some(&viewer) {
            return;
        }
        inner.owner_absent_since = None;
        let displaced = inner.owner.replace(viewer.clone());
        inner.tell(&viewer, true);
        if let Some(displaced) = displaced {
            inner.tell(&displaced, false);
        }
    }

    /// Whether `connection`'s viewer may size the panes.
    pub fn owns(&self, connection: u64) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner
            .announce
            .get(&connection)
            .is_some_and(|a| inner.owner.as_ref() == Some(&a.viewer))
    }

    /// Hand the sizing on if its owner has been gone past the grace.
    ///
    /// Called from the hubs' worker tick rather than a timer of its own: the
    /// grace only has to end promptly while someone is there to notice.
    pub fn settle(&self, now: Instant) {
        // Cheap on the common path: the owner is present, so nothing is pending.
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.expire_absent_owner(now);
    }

    /// The current owner, for tests and diagnostics.
    #[cfg(test)]
    pub(crate) fn owner(&self) -> Option<ViewerId> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .owner
            .clone()
    }
}

impl Inner {
    /// Give the sizing to the most recent viewer still here, once the owner has
    /// been without a connection for longer than the grace.
    fn expire_absent_owner(&mut self, now: Instant) {
        let Some(since) = self.owner_absent_since else {
            return;
        };
        if now.duration_since(since) < RELEASE_GRACE {
            return;
        }
        self.owner_absent_since = None;
        // The same rule that gave it away in the first place. With nobody left
        // it goes unowned and every pane keeps the size it has — there is no
        // client to fit.
        self.owner = self.arrival.last().cloned();
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

#[cfg(test)]
#[path = "size_owner_tests.rs"]
mod tests;
