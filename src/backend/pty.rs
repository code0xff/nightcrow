use super::slot::{PaneSlot, PaneSlots};
use super::{BackendEvent, PaneId, ResizeOutcome, TerminalBackend};
use crate::config::ShellConfig;
use crate::platform::threading::try_timed_join;
use anyhow::Result;
use portable_pty::PtySize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::thread;
use std::time::{Duration, Instant};

#[path = "pty_spawn.rs"]
mod spawn;

/// Reap window for PTY reader / wait threads. Bigger than the commit-log
/// REAP_TIMEOUT because `read()` on a PTY master can stay blocked if a
/// daemonized grandchild inherited the slave fd.
const PTY_REAP_TIMEOUT: Duration = Duration::from_millis(50);

/// Max events drained from any one pane in a single `drain_events` call.
const PER_PANE_DRAIN_BUDGET: usize = 64;

/// How long a pane whose child has exited keeps draining before the exit is
/// reported. The caller destroys the pane the moment it sees `Exited`, so
/// reporting at once would drop output the ConPTY host is still copying.
const EXIT_DRAIN_GRACE: Duration = Duration::from_millis(50);

pub(super) enum PtyEvent {
    Output(Vec<u8>),
    /// The pane's process is gone. Says nothing about output still in flight.
    ChildExited,
    /// End of the master: no further output can arrive.
    Exited,
}

/// How far a pane has moved through its shutdown. The two end signals are
/// not interchangeable: EOF on the master is final, but a child's death is
/// not — on Windows `ClosePseudoConsole` only runs when the master is dropped,
/// so `read()` never returns EOF and the child's exit is the *only* signal a
/// pane gets. `Draining` holds the exit back until the channel is dry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExitPhase {
    /// No exit signal seen.
    Running,
    /// The child is gone; its remaining output is still being drained.
    Draining { since: Instant },
    /// `BackendEvent::Exited` has been emitted — never emit a second one.
    Reported,
}

pub(super) struct PtyPane {
    // Option wrapping so `Drop` releases master/writer before joining the
    // reader thread, which only unblocks when both PTY sides close.
    pub(super) master: Option<Box<dyn portable_pty::MasterPty>>,
    pub(super) writer: Option<Box<dyn Write + Send>>,
    pub(super) killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    pub(super) rx: Receiver<PtyEvent>,
    pub(super) reader_handle: Option<thread::JoinHandle<()>>,
    pub(super) wait_handle: Option<thread::JoinHandle<()>>,
    pub(super) exit: ExitPhase,
}

impl Drop for PtyPane {
    fn drop(&mut self) {
        // Best-effort kill: the child may already be gone.
        let _ = self.killer.kill();
        // Drop writer/master so the reader's blocked `read()` returns EOF;
        // without this, joining the reader would hang.
        self.writer.take();
        self.master.take();
        // Bounded join: if a daemonized grandchild kept the slave fd open the
        // reader's `read()` never returns EOF — detach rather than freeze the
        // close. Inside drop, so a thread panic is logged, not propagated.
        if let Some(h) = self.reader_handle.take() {
            try_timed_join(h, PTY_REAP_TIMEOUT);
        }
        if let Some(h) = self.wait_handle.take() {
            try_timed_join(h, PTY_REAP_TIMEOUT);
        }
    }
}

pub struct PtyBackend {
    // BTreeMap so per-frame drain visits panes in PaneId order — ids are
    // monotonic, keeping the drain deterministic across runs.
    pub(super) panes: BTreeMap<PaneId, PtyPane>,
    /// Slot bookkeeping kept beside `panes` rather than inside `PtyPane`
    /// because a relaunch replaces the pane while the slot survives it.
    pub(super) slots: PaneSlots,
    pub(super) next_id: PaneId,
    // Spawned shell cwd must match the repo nightcrow tracks.
    pub(super) cwd: PathBuf,
    /// Panes created since the last drain, waiting to be reported.
    ///
    /// This backend knows the id the moment it makes the pane, but the trait
    /// reports every pane through the event stream so a caller cannot come to
    /// depend on an answer a remote backend has no way to give.
    created: Vec<BackendEvent>,
    /// The shell every terminal pane is spawned with.
    pub(super) shell: ShellConfig,
}

impl PtyBackend {
    pub fn new(cwd: impl AsRef<Path>, shell: ShellConfig) -> Self {
        Self {
            panes: BTreeMap::new(),
            slots: PaneSlots::default(),
            next_id: 1,
            cwd: cwd.as_ref().to_path_buf(),
            created: Vec::new(),
            shell,
        }
    }

    /// The slot behind a live pane, or `None` once the pane is gone.
    pub fn slot(&self, id: PaneId) -> Option<&PaneSlot> {
        self.slots.get(id)
    }

    /// Which live pane a token names.
    pub fn pane_for_token(&self, token: &super::PaneToken) -> Option<PaneId> {
        self.slots.find_by_token(token)
    }

    /// Whether the pane still has a running process.
    pub fn is_process_alive(&self, id: PaneId) -> bool {
        self.panes.contains_key(&id)
    }

    /// Let go of a pane's process while keeping its slot, so a wait can run
    /// for hours without holding the dead child's fds and threads open just
    /// to preserve its token. The slot is the only part a relaunch needs.
    pub fn release_process(&mut self, id: PaneId) {
        self.panes.remove(&id);
    }

    /// Drop a slot for good, retiring its token: the wait was abandoned, the
    /// pane was closed, or the session is going away.
    pub fn retire_slot(&mut self, id: PaneId) {
        self.slots.remove(id);
    }
}

impl TerminalBackend for PtyBackend {
    fn create_pane(&mut self, rows: u16, cols: u16, command: Option<&str>) -> Result<()> {
        let id = self.open_pane(rows, cols, command)?;
        // Queued rather than returned: the trait reports every pane the same
        // way, and this backend simply knows the answer before it queues it.
        // `requested` is always true — nothing else can create a pane here.
        self.created.push(BackendEvent::Created {
            title: None,
            pane: id,
            rows,
            cols,
            requested: true,
        });
        Ok(())
    }

    fn destroy_pane(&mut self, id: PaneId) {
        self.panes.remove(&id);
        // A relaunch keeps the slot by going through `relaunch_pane` instead
        // of destroy-then-open.
        self.slots.remove(id);
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

    fn resize(&mut self, id: PaneId, rows: u16, cols: u16) -> Result<ResizeOutcome> {
        let pane = self
            .panes
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("pane {id} not found"))?;
        let master = pane
            .master
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("pane {id} PTY master already released"))?;
        master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(ResizeOutcome::Applied)
    }

    fn drain_events(&mut self) -> Vec<BackendEvent> {
        // Pane removal is the caller's responsibility (App::poll_terminal calls
        // destroy_pane on Exited). Doing it here too created a dual-ownership
        // race where reader-thread events queued after destroy_pane were
        // silently dropped, and where Exited could be reported twice.
        //
        // The reader thread emits all Output messages, then a single Exited as
        // the last message before its sender drops, so the mpsc order means no
        // separate post-Exited drain is needed. `ChildExited` carries no such
        // ordering — it can overtake output already written — so it only moves
        // the pane to `Draining`.
        //
        // Each pane is drained up to PER_PANE_DRAIN_BUDGET events so one noisy
        // pane (e.g. `yes | head -100000`) cannot starve its siblings within a
        // frame; the rest lands on the next tick.
        // Ahead of any output: a pane has to exist before bytes can be routed
        // to it, and both can be queued before the same drain.
        let mut events: Vec<BackendEvent> = std::mem::take(&mut self.created);
        let now = Instant::now();
        for (id, pane) in &mut self.panes {
            let mut budget = PER_PANE_DRAIN_BUDGET;
            while budget > 0 {
                match pane.rx.try_recv() {
                    Ok(PtyEvent::Output(data)) => {
                        // One timestamp for the whole drain: the bytes arrived
                        // between this tick and the last, and a per-event clock
                        // read would claim a precision the 8 ms poll does not
                        // have.
                        self.slots.mark_output(*id, now);
                        events.push(BackendEvent::Output { pane: *id, data });
                    }
                    Ok(PtyEvent::ChildExited) => {
                        if pane.exit == ExitPhase::Running {
                            pane.exit = ExitPhase::Draining { since: now };
                        }
                    }
                    Ok(PtyEvent::Exited) => {
                        if pane.exit != ExitPhase::Reported {
                            pane.exit = ExitPhase::Reported;
                            events.push(BackendEvent::Exited { pane: *id });
                        }
                        break;
                    }
                    Err(_) => {
                        // Dry: for a dead child past its drain grace, the cue
                        // to report.
                        if let ExitPhase::Draining { since } = pane.exit
                            && now.duration_since(since) >= EXIT_DRAIN_GRACE
                        {
                            pane.exit = ExitPhase::Reported;
                            events.push(BackendEvent::Exited { pane: *id });
                        }
                        break;
                    }
                }
                budget -= 1;
            }
        }
        events
    }
}

// No explicit `Drop` on `PtyBackend` on purpose: the map drops every pane and
// `PtyPane::drop` handles kill+release+join — an empty Drop would obscure that.

#[cfg(test)]
#[path = "pty_tests.rs"]
mod tests;
