use super::slot::{PaneSlot, PaneSlots};
use super::{BackendEvent, PaneId, TerminalBackend};
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
/// daemonized grandchild inherited the slave fd — portable_pty does not
/// guarantee CLOEXEC on every platform, so the join must be detachable.
const PTY_REAP_TIMEOUT: Duration = Duration::from_millis(50);

/// Max events drained from any one pane in a single `drain_events` call.
/// A pane that produces output faster than the UI loop consumes it would
/// otherwise monopolize the per-frame drain and starve sibling panes —
/// the round-robin cap bounds the work per pane to a small constant.
const PER_PANE_DRAIN_BUDGET: usize = 64;

pub(super) enum PtyEvent {
    Output(Vec<u8>),
    Exited,
}

pub(super) struct PtyPane {
    // master/writer are wrapped in Option so `Drop` can release them
    // before joining the reader thread — the reader blocks in `read()`
    // and only unblocks when both sides of the PTY are closed.
    pub(super) master: Option<Box<dyn portable_pty::MasterPty>>,
    pub(super) writer: Option<Box<dyn Write + Send>>,
    pub(super) killer: Box<dyn portable_pty::ChildKiller + Send + Sync>,
    pub(super) rx: Receiver<PtyEvent>,
    pub(super) reader_handle: Option<thread::JoinHandle<()>>,
    pub(super) wait_handle: Option<thread::JoinHandle<()>>,
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
        // Bounded join so closing a pane cannot leave reader/wait threads
        // alive holding fds against the (possibly killed) child. If a
        // daemonized grandchild kept the slave fd open, the reader's
        // `read()` won't return EOF; detach in that case rather than
        // freezing the close. We're inside drop, so a panic in either
        // thread is logged rather than propagated.
        if let Some(h) = self.reader_handle.take() {
            try_timed_join(h, PTY_REAP_TIMEOUT);
        }
        if let Some(h) = self.wait_handle.take() {
            try_timed_join(h, PTY_REAP_TIMEOUT);
        }
    }
}

pub struct PtyBackend {
    // BTreeMap (not HashMap) so per-frame event drain visits panes in
    // PaneId order — IDs are monotonic, so this matches creation order
    // and stays deterministic across runs. HashMap iteration was random
    // per process, which made inter-pane event ordering unobservable
    // and could mask fairness regressions in tests.
    pub(super) panes: BTreeMap<PaneId, PtyPane>,
    /// Slot bookkeeping — identity, launch, idle clock — kept beside `panes`
    /// rather than inside `PtyPane` because a relaunch replaces the `PtyPane`
    /// while the slot has to survive it.
    pub(super) slots: PaneSlots,
    pub(super) next_id: PaneId,
    // Each new pane spawns the shell here so its cwd matches the repo
    // nightcrow is tracking, even when the binary was launched elsewhere.
    pub(super) cwd: PathBuf,
    /// Panes created since the last drain, waiting to be reported.
    ///
    /// This backend knows the id the moment it makes the pane, but the trait
    /// reports every pane through the event stream so a caller cannot come to
    /// depend on an answer a remote backend has no way to give.
    created: Vec<BackendEvent>,
}

impl PtyBackend {
    pub fn new(cwd: impl AsRef<Path>) -> Self {
        Self {
            panes: BTreeMap::new(),
            slots: PaneSlots::default(),
            next_id: 1,
            cwd: cwd.as_ref().to_path_buf(),
            created: Vec::new(),
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

    /// Let go of a pane's process while keeping its slot.
    ///
    /// Splitting this out of `destroy_pane` is what makes waiting for a reset
    /// affordable: a wait can run for hours, and holding the dead child's fds
    /// and threads open for that long to preserve the token would be pure
    /// waste. The slot is small, and it is the only part a relaunch needs.
    pub fn release_process(&mut self, id: PaneId) {
        self.panes.remove(&id);
    }

    /// Drop a slot for good, retiring its token.
    ///
    /// Called when nothing more is expected of the pane — the wait was
    /// abandoned, the pane was closed, or the session is going away.
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
            // A local backend has no name to give: whoever opened the pane knows
            // what it is for.
            title: None,
            pane: id,
            rows,
            cols,
            requested: true,
        });
        Ok(())
    }

    fn destroy_pane(&mut self, id: PaneId) {
        // Removing the pane drops it, which runs PtyPane::drop: kill,
        // release master/writer, join reader/wait threads.
        self.panes.remove(&id);
        // The slot goes with it, retiring its token. A relaunch keeps the slot
        // by going through `relaunch_pane` instead of destroy-then-open.
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
        // Ahead of any output: a pane has to exist before bytes can be routed
        // to it, and both can be queued before the same drain.
        let mut events: Vec<BackendEvent> = std::mem::take(&mut self.created);
        let now = Instant::now();
        for (id, pane) in &self.panes {
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
#[path = "pty_tests.rs"]
mod tests;
