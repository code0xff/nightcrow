use super::{BackendEvent, PaneId, TerminalBackend};
use anyhow::Result;
use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// Max events drained from any one pane in a single `drain_events` call.
/// A pane that produces output faster than the UI loop consumes it would
/// otherwise monopolize the per-frame drain and starve sibling panes —
/// the round-robin cap bounds the work per pane to a small constant.
const PER_PANE_DRAIN_BUDGET: usize = 64;

enum PtyEvent {
    Output(Vec<u8>),
    Exited,
}

struct PtyPane {
    // master/writer are wrapped in Option so `Drop` can release them
    // before joining the reader thread — the reader blocks in `read()`
    // and only unblocks when both sides of the PTY are closed.
    master: Option<Box<dyn portable_pty::MasterPty>>,
    writer: Option<Box<dyn Write + Send>>,
    killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    rx: Receiver<PtyEvent>,
    reader_handle: Option<thread::JoinHandle<()>>,
    wait_handle: Option<thread::JoinHandle<()>>,
}

impl Drop for PtyPane {
    fn drop(&mut self) {
        // Best-effort kill: the child may already be gone.
        let _ = self.killer.kill();
        // Drop writer/master so the reader's blocked `read()` returns EOF
        // and the thread exits. Without this, joining the reader would
        // hang.
        self.writer.take();
        self.master.take();
        // Join so closing a pane cannot leave reader/wait threads alive
        // holding fds against the (possibly killed) child. We're inside
        // drop, so a panic in either thread is logged rather than
        // propagated.
        if let Some(h) = self.reader_handle.take()
            && let Err(e) = h.join()
        {
            tracing::warn!(?e, "PTY reader thread panicked during shutdown");
        }
        if let Some(h) = self.wait_handle.take()
            && let Err(e) = h.join()
        {
            tracing::warn!(?e, "PTY wait thread panicked during shutdown");
        }
    }
}

pub struct PtyBackend {
    // BTreeMap (not HashMap) so per-frame event drain visits panes in
    // PaneId order — IDs are monotonic, so this matches creation order
    // and stays deterministic across runs. HashMap iteration was random
    // per process, which made inter-pane event ordering unobservable
    // and could mask fairness regressions in tests.
    panes: BTreeMap<PaneId, PtyPane>,
    next_id: PaneId,
    // Each new pane spawns the shell here so its cwd matches the repo
    // nightcrow is tracking, even when the binary was launched elsewhere.
    cwd: PathBuf,
}

impl PtyBackend {
    pub fn new(cwd: impl AsRef<Path>) -> Self {
        Self {
            panes: BTreeMap::new(),
            next_id: 1,
            cwd: cwd.as_ref().to_path_buf(),
        }
    }
}

impl TerminalBackend for PtyBackend {
    fn create_pane(&mut self, rows: u16, cols: u16) -> Result<PaneId> {
        // Reserve the next id only after every fallible PTY/spawn step succeeds,
        // so a failure here does not consume an id slot.
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

        let id = self.next_id;
        let next = id
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("pane id counter overflow"))?;
        self.next_id = next;

        let (tx, rx) = mpsc::channel();
        let reader_handle = thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(PtyEvent::Output(buf[..n].to_vec())).is_err() {
                            break;
                        }
                    }
                }
            }
            let _ = tx.send(PtyEvent::Exited);
        });

        let wait_handle = thread::spawn(move || {
            let _ = child.wait();
        });

        self.panes.insert(
            id,
            PtyPane {
                master: Some(pair.master),
                writer: Some(writer),
                killer,
                rx,
                reader_handle: Some(reader_handle),
                wait_handle: Some(wait_handle),
            },
        );
        Ok(id)
    }

    fn destroy_pane(&mut self, id: PaneId) {
        // Removing the pane drops it, which runs PtyPane::drop: kill,
        // release master/writer, join reader/wait threads.
        self.panes.remove(&id);
    }

    fn send_input(&mut self, id: PaneId, data: &[u8]) -> Result<()> {
        // Surface "no such pane" as an error so the caller can warn — a
        // silent Ok hid drops where the UI kept the pane in `panes` but the
        // backend had already discarded it.
        let pane = self
            .panes
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("pane {id} not found"))?;
        let writer = pane
            .writer
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("pane {id} writer already released"))?;
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) {
        if let Some(pane) = self.panes.get_mut(&id)
            && let Some(master) = pane.master.as_mut()
        {
            let _ = master.resize(PtySize {
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
        // Pane removal is the caller's responsibility (App::poll_terminal calls
        // destroy_pane on Exited). Doing it here too created a dual-ownership
        // race where reader-thread events queued after destroy_pane were
        // silently dropped, and where Exited could be reported twice.
        //
        // The reader thread emits all Output messages, then a single Exited as
        // the last message before its sender drops. The mpsc channel preserves
        // send order, so any Output enqueued before Exited has already been
        // surfaced by an earlier iteration of the outer try_recv loop — no
        // separate post-Exited drain is needed.
        //
        // Each pane is drained up to PER_PANE_DRAIN_BUDGET events to keep
        // one noisy pane (e.g. `yes | head -100000`) from starving its
        // siblings within a single frame; whatever is left lands on the
        // next tick.
        let mut events = Vec::new();
        for (id, pane) in &self.panes {
            let mut budget = PER_PANE_DRAIN_BUDGET;
            while budget > 0 {
                match pane.rx.try_recv() {
                    Ok(PtyEvent::Output(data)) => {
                        events.push(BackendEvent::Output { pane: *id, data });
                    }
                    Ok(PtyEvent::Exited) => {
                        events.push(BackendEvent::Exited { pane: *id });
                        break;
                    }
                    Err(_) => break,
                }
                budget -= 1;
            }
        }
        events
    }
}

// `PtyBackend` no longer needs an explicit Drop: `HashMap::drop` drops every
// pane, and `PtyPane::drop` handles kill+release+join. Leaving an empty
// Drop here would still work but would obscure that ownership.

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
