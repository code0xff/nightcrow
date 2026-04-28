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

pub struct BackendSelection {
    pub backend: Box<dyn TerminalBackend>,
    pub warning: Option<String>,
}

pub fn select_backend() -> Result<BackendSelection> {
    if tmux::is_available() {
        match TmuxBackend::new() {
            Ok(backend) => {
                return Ok(BackendSelection {
                    backend: Box::new(backend),
                    warning: None,
                });
            }
            Err(err) => {
                return Ok(BackendSelection {
                    backend: Box::new(PtyBackend::new()),
                    warning: Some(format!(
                        "tmux backend unavailable: {err}; using PTY fallback"
                    )),
                });
            }
        }
    }

    Ok(BackendSelection {
        backend: Box::new(PtyBackend::new()),
        warning: Some("tmux not found; using PTY fallback".to_string()),
    })
}
