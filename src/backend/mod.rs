pub mod hub;
pub mod pty;

pub use hub::HubBackend;
pub use pty::PtyBackend;

use anyhow::Result;

pub type PaneId = u32;

#[derive(Debug)]
pub enum BackendEvent {
    /// A pane now exists. Reported rather than returned from `create_pane`
    /// because a backend serving a shared session cannot answer on the spot:
    /// the id comes from wherever the PTY actually lives, and a pane another
    /// client created arrives the same way, with nothing here having asked.
    Created {
        pane: PaneId,
        rows: u16,
        cols: u16,
        /// Whether this side asked for it. A pane someone else opened must not
        /// take the focus away from what this client is looking at — which tab
        /// and pane a client sits on is its own business.
        requested: bool,
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
    /// client owns the sizing, and one request can be clamped. An emulator has
    /// to wrap where the child does, so this is what it follows.
    Resized {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    /// Whether this side is the one whose layout sets the pane sizes.
    ///
    /// A PTY has one size and a child cannot be re-flowed after the fact, so
    /// one client decides it and the rest watch. Owning a local `PtyBackend`
    /// means always owning the sizing, which is why nothing reports it there.
    SizeOwnership {
        owned: bool,
    },
}

pub trait TerminalBackend {
    /// Ask for a pane sized `rows`x`cols`. When `command` is `Some`, the
    /// pane's shell runs that command immediately (via `$SHELL -lc <command>`);
    /// `None` spawns a bare interactive shell.
    ///
    /// The pane arrives as [`BackendEvent::Created`], not as a return value.
    /// `Ok` means the request was made, and an error means it could not be —
    /// neither says the pane exists yet.
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<()>;
    fn destroy_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()>;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16);
    fn drain_events(&mut self) -> Vec<BackendEvent>;

    /// Ask to become the client whose layout sets the pane sizes.
    ///
    /// A no-op by default: a backend that owns its PTYs is the only client they
    /// have, so there is nobody to take them from. Only a backend serving a
    /// shared session has anything to ask.
    fn claim_size(&mut self) {}

    /// Test hook: byte payloads recorded by a recording backend. Real
    /// backends return `None`; the in-memory test `FakeBackend` overrides
    /// this so input tests can assert exact PTY pass-through bytes.
    #[cfg(test)]
    fn test_sent_payloads(&self) -> Option<Vec<Vec<u8>>> {
        None
    }
}
