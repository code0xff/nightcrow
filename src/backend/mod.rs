pub mod pty;

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

    /// Test hook: byte payloads recorded by a recording backend. Real
    /// backends return `None`; the in-memory test `FakeBackend` overrides
    /// this so input tests can assert exact PTY pass-through bytes.
    #[cfg(test)]
    fn test_sent_payloads(&self) -> Option<Vec<Vec<u8>>> {
        None
    }
}
