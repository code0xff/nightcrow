pub mod hub;
pub mod identity;
pub mod pty;
pub mod slot;

pub use hub::HubBackend;
pub use identity::{PaneGeneration, PaneToken};
pub use pty::PtyBackend;

use anyhow::Result;

pub type PaneId = u32;

#[derive(Debug)]
pub enum BackendEvent {
    /// A pane now exists. Reported rather than returned from `create_pane`
    /// because a backend serving a shared session cannot answer on the spot:
    /// the id comes from wherever the PTY actually lives.
    Created {
        pane: PaneId,
        rows: u16,
        cols: u16,
        /// Whether this side asked for it. A pane someone else opened must not
        /// take the focus away from what this client is looking at.
        requested: bool,
        /// The name the session gives the pane, which only a configured startup
        /// terminal has. `None` leaves the naming to this client.
        title: Option<String>,
    },
    Output {
        pane: PaneId,
        data: Vec<u8>,
    },
    Exited {
        pane: PaneId,
    },
    /// The size a pane's PTY is now set to.
    ///
    /// Only a backend serving a shared session reports this, and it is not
    /// necessarily what this side asked for: the size belongs to whichever
    /// client owns the sizing. An emulator has to wrap where the child does.
    Resized {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    /// The canonical order of the panes.
    ///
    /// Only a backend serving a shared session reports this: the order is part
    /// of what the session owns. Ids this side does not know are ignored and
    /// panes the order omits keep their place.
    Reordered {
        order: Vec<PaneId>,
    },
    /// Whether this side is the one whose layout sets the pane sizes.
    ///
    /// A PTY has one size and a child cannot be re-flowed after the fact, so
    /// one client decides it and the rest watch.
    SizeOwnership {
        owned: bool,
    },
    /// What a plugin driving `pane` reports about getting it running again.
    ///
    /// Only a backend serving a shared session reports this: the plugins run
    /// beside the session's panes, not beside this client.
    Recovery {
        pane: PaneId,
        /// The plugin's own short label. Uninterpreted here; the one value with a
        /// meaning is `"cancelled"`, which ends the report.
        state: String,
        detail: Option<String>,
        /// When the wait ends, in unix epoch seconds, or `None` when no clock is
        /// involved.
        deadline_epoch: Option<i64>,
        attempt: u32,
    },
}

pub trait TerminalBackend {
    /// Ask for a pane sized `rows`x`cols`. When `command` is `Some`, the
    /// pane's shell runs that command immediately; `None` spawns a bare
    /// interactive shell.
    ///
    /// The pane arrives as [`BackendEvent::Created`], not as a return value.
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<()>;
    fn destroy_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()>;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16);
    fn drain_events(&mut self) -> Vec<BackendEvent>;

    /// Ask for the panes to be put in this order.
    ///
    /// A no-op by default: a backend that owns its panes has no one to negotiate
    /// the order with. A shared session answers with [`BackendEvent::Reordered`].
    fn reorder(&mut self, order: &[PaneId]) {
        let _ = order;
    }

    /// Ask to become the client whose layout sets the pane sizes.
    ///
    /// A no-op by default: a backend that owns its PTYs is the only client they
    /// have. Only a backend serving a shared session has anything to ask.
    fn claim_size(&mut self) {}

    /// Ask the session to give up on a pane's pending recovery.
    ///
    /// A no-op by default: a backend that owns its PTYs has no plugin nursing one
    /// back, so there is nothing pending to abandon. The session answers with a
    /// [`BackendEvent::Recovery`] whose state is `"cancelled"`.
    fn cancel_recovery(&mut self, pane: PaneId) {
        let _ = pane;
    }

    /// Test hook: byte payloads recorded by a recording backend. Real
    /// backends return `None`; the in-memory test `FakeBackend` overrides
    /// this so input tests can assert exact PTY pass-through bytes.
    #[cfg(test)]
    fn test_sent_payloads(&self) -> Option<Vec<Vec<u8>>> {
        None
    }
}
