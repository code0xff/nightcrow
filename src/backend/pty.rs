use super::{BackendEvent, PaneId, TerminalBackend};
use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

enum PtyEvent {
    Output(Vec<u8>),
    Exited,
}

struct PtyPane {
    master: Box<dyn portable_pty::MasterPty>,
    writer: Box<dyn Write + Send>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    rx: Receiver<PtyEvent>,
}

pub struct PtyBackend {
    panes: HashMap<PaneId, PtyPane>,
    next_id: PaneId,
    // Each new pane spawns the shell here so its cwd matches the repo
    // nightcrow is tracking, even when the binary was launched elsewhere.
    cwd: PathBuf,
}

impl PtyBackend {
    pub fn new(cwd: impl AsRef<Path>) -> Self {
        Self {
            panes: HashMap::new(),
            next_id: 1,
            cwd: cwd.as_ref().to_path_buf(),
        }
    }
}

impl TerminalBackend for PtyBackend {
    fn create_pane(&mut self, rows: u16, cols: u16) -> Result<PaneId> {
        let id = self.next_id;
        self.next_id = id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("pane id counter overflow"))?;

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
        // Only set cwd if the directory actually exists; otherwise inherit
        // ours so spawn does not fail outright (matters for unit tests that
        // pass placeholder paths).
        if let Ok(canonical) = self.cwd.canonicalize() {
            cmd.cwd(canonical);
        }
        let mut child = pair.slave.spawn_command(cmd)?;
        let killer = child.clone_killer();
        drop(pair.slave);

        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (tx, rx) = mpsc::channel();
        let output_tx = tx.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if output_tx.send(PtyEvent::Output(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = output_tx.send(PtyEvent::Exited);
        });

        thread::spawn(move || {
            let _ = child.wait();
        });

        self.panes.insert(
            id,
            PtyPane {
                master: pair.master,
                writer,
                killer,
                rx,
            },
        );
        Ok(id)
    }

    fn destroy_pane(&mut self, id: PaneId) {
        if let Some(mut pane) = self.panes.remove(&id) {
            let _ = pane.killer.kill();
        }
    }

    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()> {
        if let Some(pane) = self.panes.get_mut(&id) {
            pane.writer.write_all(data)?;
            pane.writer.flush()?;
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

    fn set_cwd(&mut self, path: &std::path::Path) {
        self.cwd = path.to_path_buf();
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        let mut events = Vec::new();
        let mut exited = Vec::new();
        for (id, pane) in &self.panes {
            while let Ok(event) = pane.rx.try_recv() {
                match event {
                    PtyEvent::Output(data) => events.push(BackendEvent::Output { pane: *id, data }),
                    PtyEvent::Exited => {
                        // Drain any output buffered between the last read and the
                        // exit signal before advertising the pane as gone.
                        while let Ok(PtyEvent::Output(data)) = pane.rx.try_recv() {
                            events.push(BackendEvent::Output { pane: *id, data });
                        }
                        events.push(BackendEvent::Exited { pane: *id });
                        exited.push(*id);
                        break;
                    }
                }
            }
        }
        for id in exited {
            self.panes.remove(&id);
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn pty_backend_create_and_destroy_pane() {
        let mut backend = PtyBackend::new(".");
        let id = backend.create_pane(24, 80).expect("create_pane failed");
        assert_eq!(id, 1);
        backend.destroy_pane(id);
        assert!(!backend.panes.contains_key(&id));
    }

    #[test]
    fn pty_backend_drains_output_before_exit_event() {
        let mut backend = PtyBackend::new(".");
        let id = backend.create_pane(24, 80).expect("create_pane failed");

        backend
            .send_input(id, b"printf nightcrow-pty-output; exit\n")
            .expect("send_input failed");

        let deadline = Instant::now() + Duration::from_secs(3);
        let mut output = Vec::new();
        let mut saw_exit = false;
        while Instant::now() < deadline {
            for event in backend.drain_events() {
                match event {
                    BackendEvent::Output { data, .. } => output.extend(data),
                    BackendEvent::Exited { pane } if pane == id => saw_exit = true,
                    BackendEvent::Exited { .. } => {}
                }
            }
            if saw_exit {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(saw_exit, "PTY did not exit before timeout");
        assert!(
            String::from_utf8_lossy(&output).contains("nightcrow-pty-output"),
            "PTY output was not drained before exit"
        );
    }
}
