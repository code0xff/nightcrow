//! The hub's end of the session's size ownership.
//!
//! The rules and the state live one level up, in
//! [`crate::session::size_owner`]: which screen the panes are fitted to is
//! the session's answer, because every client shows the same repository. What is
//! left here is the two places a hub touches it — a client asking for the sizing,
//! and the worker letting a departed owner's grace run out.

use super::TerminalHub;

impl TerminalHub {
    /// Take the sizing for `connection`, at its own request.
    ///
    /// Unlike connecting, this is unconditional: it is how a client that has
    /// been here all along says the panes should fit *its* screen (the viewer's
    /// fit button, the TUI's chord).
    pub(super) fn claim_size(&self, connection: u64) {
        self.ownership.claim(connection, std::time::Instant::now());
    }

    /// Let the sizing pass on if its owner has been gone past the grace.
    ///
    /// Driven from the worker tick rather than a timer of its own: the grace only
    /// has to end promptly while there is a hub running for someone to see it,
    /// and it costs a lock and an early return on the common path.
    pub(super) fn settle_size_owner(&self, now: std::time::Instant) {
        self.ownership.settle(now);
    }

    /// Whether `connection` may size this repository's panes.
    pub(super) fn owns_size(&self, connection: u64) -> bool {
        self.ownership.owns(connection)
    }

    /// This hub client's ownership registration, or `None` once it has gone.
    ///
    /// The two ids are separate on purpose (see [`Client::connection`]), and a
    /// command carries the hub's — it was queued by a connection thread and the
    /// worker reads it a tick later, by which time that connection may be gone.
    ///
    /// [`Client::connection`]: super::session::Client::connection
    pub(super) fn connection_of(&self, client: u64) -> Option<u64> {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
            .iter()
            .find(|c| c.id == client)
            .map(|c| c.connection)
    }
}
