//! Undoing a connection that [`TerminalHub::connect`] registered but never hands a
//! session to.

use super::TerminalHub;
use std::time::Instant;

/// Removes a client `connect` registered but will never hand a session to.
///
/// Registration and the session that unregisters it are separated by the whole
/// replay, so a panic in between would leave the client in the broadcast list
/// with its replay gate closed: nothing would ever open the gate, every later
/// frame would be held until the client overflowed and was cut off, and its size
/// registration would stay behind for good. Disarmed once the session exists.
pub(super) struct RegistrationGuard<'a> {
    hub: &'a TerminalHub,
    id: u64,
    /// The size-ownership registration this connection made, once it has one.
    connection: Option<u64>,
    armed: bool,
}

impl<'a> RegistrationGuard<'a> {
    /// Arm the guard for a client `connect` has just registered.
    pub(super) fn new(hub: &'a TerminalHub, id: u64) -> Self {
        Self {
            hub,
            id,
            connection: None,
            armed: true,
        }
    }

    /// Record the size registration, so an unwind releases that too.
    pub(super) fn registered(&mut self, connection: u64) {
        self.connection = Some(connection);
    }

    /// The connection completed: from here its session owns the registration.
    pub(super) fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for RegistrationGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // A poisoned lock is taken anyway: this runs while unwinding, and a
        // second panic here would abort the process instead of unwinding it.
        {
            let mut state = self
                .hub
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.clients.retain(|client| client.id != self.id);
        }
        // Off the hub's lock, in the order `disconnect` uses: the sizing is the
        // session's business and may have to tell clients on other hubs.
        if let Some(connection) = self.connection {
            self.hub.ownership.leave(connection, Instant::now());
            self.hub.discard_pending_resizes(connection);
        }
    }
}
