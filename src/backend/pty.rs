use super::{BackendEvent, BackendKind, PaneId, TerminalBackend};
use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::mpsc::{self, Receiver};
use std::thread;

struct PtyPane {
    master: Box<dyn portable_pty::MasterPty>,
    writer: Box<dyn Write + Send>,
    rx: Receiver<Vec<u8>>,
}

pub struct PtyBackend {
    panes: HashMap<PaneId, PtyPane>,
    next_id: PaneId,
}

impl PtyBackend {
    pub fn new() -> Self {
        Self {
            panes: HashMap::new(),
            next_id: 1,
        }
    }
}

impl TerminalBackend for PtyBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Pty
    }

    fn create_pane(&mut self, rows: u16, cols: u16) -> Result<PaneId> {
        let id = self.next_id;
        self.next_id += 1;

        let pty_system = NativePtySystem::default();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        cmd.env("TERM", "xterm-256color");
        let _child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        self.panes.insert(id, PtyPane { master: pair.master, writer, rx });
        Ok(id)
    }

    fn destroy_pane(&mut self, id: PaneId) {
        self.panes.remove(&id);
    }

    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()> {
        if let Some(pane) = self.panes.get_mut(&id) {
            pane.writer.write_all(data)?;
        }
        Ok(())
    }

    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) {
        if let Some(pane) = self.panes.get_mut(&id) {
            let _ = pane.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        for (id, pane) in &self.panes {
            while let Ok(data) = pane.rx.try_recv() {
                events.push(BackendEvent::Output { pane: *id, data });
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_backend_create_and_destroy_pane() {
        let mut backend = PtyBackend::new();
        let id = backend.create_pane(24, 80).expect("create_pane failed");
        assert_eq!(id, 1);
        backend.destroy_pane(id);
        assert!(!backend.panes.contains_key(&id));
    }
}
