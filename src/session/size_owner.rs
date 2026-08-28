//! Which screen the session's PTYs are fitted to.
//!
//! A PTY is a contract with a child process: the child draws for the width it
//! was told, and nothing can re-flow an alternate-screen program afterwards. So
//! the size is one value with one owner — the most recent viewer to arrive
//! (tmux's `window-size latest`), until another takes it.
//!
//! **Why this is the session's and not each hub's.** Which repository is in
//! front is shared by the whole session, so "which screen is this session fitted
//! to" is one question. Asked per hub, it was re-answered on every switch —
//! moving tabs made every attached page reconnect at once and the sizing fell
//! to whichever handshake finished last.
//!
//! **A viewer is not a connection.** A socket opens for reasons that are not a
//! person sitting down: a repository switch, a page reload, a network blip. So
//! a viewer names itself ([`ViewerId`]) and says outright whether it is newly
//! arrived; connections come and go beneath a viewer without moving anything.
//!
//! **Unowned means empty.** The sizing has no owner only while nobody is here.
//! A session with a person in it and nobody sizing for them renders their panes
//! at a departed screen's size — the state a phone produced every time it woke
//! up.
//!
//! This file is the facade — locking, and the contract each caller sees. The
//! rules themselves live with the state they read, in [`state`].

use crate::session::terminal::frame::TerminalFrame;
use std::sync::mpsc::SyncSender;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

#[path = "size_owner_audit.rs"]
mod audit;
#[path = "size_owner_state.rs"]
mod state;

use state::Inner;

/// How long the sizing is held for an owner that has no connection left.
///
/// Switching repositories closes one terminal socket and opens another, and
/// for the moment in between the owner is connected to nothing. Handing the
/// sizing away there and back would re-fit every pane twice for a viewer that
/// never went anywhere. Only the *release* is delayed; nothing claims by
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
    #[cfg(test)]
    pub owned: bool,
}

impl SizeOwnership {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// A poisoned lock is taken anyway: the sizing is a preference about which
    /// screen to fit, and refusing to answer would take the terminals down with
    /// it.
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
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
        self.lock().join(viewer, arriving, tx, now)
    }

    /// Drop a connection. The sizing moves only when its viewer has no other,
    /// and even then not at once — see [`RELEASE_GRACE`].
    pub fn leave(&self, connection: u64, now: Instant) {
        self.lock().leave(connection, now);
    }

    /// Take the sizing at a viewer's own request — the fit button, or the TUI's
    /// chord. Unlike arriving, this is unconditional.
    pub fn claim(&self, connection: u64, now: Instant) {
        self.lock().claim(connection, now);
    }

    /// Whether `connection`'s viewer may size the panes.
    pub fn owns(&self, connection: u64) -> bool {
        self.lock().owns(connection)
    }

    /// Hand the sizing on if its owner has been gone past the grace.
    ///
    /// Called from the hubs' worker tick rather than a timer of its own: the
    /// grace only has to end promptly while someone is there to notice.
    pub fn settle(&self, now: Instant) {
        // Cheap on the common path: the owner is present, so nothing is pending.
        self.lock().expire_absent_owner(now);
    }

    /// The current owner, for tests and diagnostics.
    #[cfg(test)]
    pub(crate) fn owner(&self) -> Option<ViewerId> {
        self.lock().owner()
    }
}

#[cfg(test)]
#[path = "size_owner_tests.rs"]
mod tests;
