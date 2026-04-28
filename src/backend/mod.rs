pub mod pty;
pub mod tmux;

pub use pty::PtyBackend;
pub use tmux::TmuxBackend;

use anyhow::Result;

pub type PaneId = u32;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackendKind {
    Tmux,
    Pty,
}

#[derive(Debug)]
pub enum BackendEvent {
    Output { pane: PaneId, data: Vec<u8> },
    Exited { pane: PaneId },
}

pub trait TerminalBackend {
    fn kind(&self) -> BackendKind;
    fn create_pane(&mut self, rows: u16, cols: u16) -> Result<PaneId>;
    #[allow(dead_code)]
    fn destroy_pane(&mut self, id: PaneId);
    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()>;
    fn resize(&mut self, id: PaneId, rows: u16, cols: u16);
    fn drain_events(&mut self) -> Vec<BackendEvent>;
}

pub fn select_backend() -> Result<Box<dyn TerminalBackend>> {
    if tmux::is_available() && let Ok(b) = TmuxBackend::new() {
        return Ok(Box::new(b));
    }
    Ok(Box::new(PtyBackend::new()))
}
