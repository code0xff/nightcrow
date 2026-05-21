pub mod pty;

pub use pty::PtyBackend;

use anyhow::Result;

pub type PaneId = u32;

#[derive(Debug)]
pub enum BackendEvent {
    Output { pane: PaneId, data: Vec<u8> },
    Exited { pane: PaneId },
}

pub trait TerminalBackend {
    /// Create a pane sized `rows`x`cols`. When `command` is `Some`, the pane's
    /// shell runs that command immediately (via `$SHELL -lc <command>`);
    /// `None` spawns a bare interactive shell.
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<PaneId>;
    fn destroy_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()>;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16);
    fn drain_events(&mut self) -> Vec<BackendEvent>;
    fn set_cwd(&mut self, path: &std::path::Path);
}
