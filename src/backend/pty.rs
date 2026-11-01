use super::slot::{PaneSlot, PaneSlots};
use super::{BackendEvent, PaneId, ResizeOutcome, TerminalBackend};
use crate::config::ShellConfig;
use crate::platform::process_tree::ProcessTree;
use crate::platform::threading::WakeHandle;
use anyhow::Result;
use portable_pty::PtySize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
#[cfg(test)]
use std::sync::{Arc, atomic::AtomicBool};
use std::thread;
use std::time::{Duration, Instant};

#[path = "pty_io.rs"]
mod io;
#[path = "pty_spawn.rs"]
mod spawn;

#[path = "pty_drain.rs"]
mod drain;
mod reap;
#[path = "pty_wakeup.rs"]
mod wakeup;

/// Reap window for PTY reader / wait threads. Bigger than the commit-log
/// REAP_TIMEOUT because `read()` on a PTY master can stay blocked if a
/// daemonized grandchild inherited the slave fd.
const PTY_REAP_TIMEOUT: Duration = Duration::from_millis(50);

/// Max events drained from any one pane in a single `drain_events` call.
const PER_PANE_DRAIN_BUDGET: usize = 64;

/// The window a woken owner's per-pane budget applies to. An owner that polled on
/// a fixed tick took at most one budget per tick. An owner woken by the reader
/// runs a pass per queued event, so a per-pass budget never binds and a burst
/// reaches its clients as fast as the PTY produces it, flooding their bounded
/// queues, which cut a client off rather than skip bytes. Counting the budget
/// across every pass in this window keeps the polled ceiling.
const DRAIN_WINDOW: Duration = Duration::from_millis(8);

/// A woken owner's budget use for one pane within the current [`DRAIN_WINDOW`].
#[derive(Clone, Copy, Debug)]
pub(super) struct DrainWindow {
    pub(super) start: Instant,
    pub(super) used: usize,
}

impl DrainWindow {
    pub(super) fn end(&self) -> Instant {
        self.start + DRAIN_WINDOW
    }
}

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
    // Option wrapping lets Drop release the receiver before the process is
    // terminated, and hand the remaining handles to a reaper.
    pub(super) master: Option<Box<dyn portable_pty::MasterPty>>,
    input: Option<io::InputQueue>,
    pub(super) killer: Option<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
    pub(super) process_tree: Option<ProcessTree>,
    pub(super) rx: Option<Receiver<PtyEvent>>,
    pub(super) reader_handle: Option<thread::JoinHandle<()>>,
    pub(super) writer_handle: Option<thread::JoinHandle<()>>,
    pub(super) wait_handle: Option<thread::JoinHandle<()>>,
    #[cfg(test)]
    pub(super) output_backpressured: Arc<AtomicBool>,
    pub(super) exit: ExitPhase,
    /// A woken owner's budget use in the current window; `None` for a polling
    /// owner, whose own tick paces it.
    pub(super) window: Option<DrainWindow>,
}

impl Drop for PtyPane {
    fn drop(&mut self) {
        // A full synchronous output channel must not hold the reader while the
        // PTY is being torn down. A disconnected reader keeps draining to EOF.
        drop(self.rx.take());
        // Cancel queued input before terminating the process; a blocked
        // in-flight write is handled by the reaper's bounded worker join.
        drop(self.input.take());
        let mut reaper = reap::PaneReaper {
            master: self.master.take(),
            killer: self.killer.take(),
            process_tree: self.process_tree.take(),
            writer: self.writer_handle.take(),
            reader: self.reader_handle.take(),
            waiter: self.wait_handle.take(),
        };
        reaper.terminate();
        crate::platform::threading::spawn_reaper("pane-reaper", move || reaper.reap());
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
    /// Set by an owner that parks between passes, so the reader and waiter
    /// threads below can tell it an event is queued. `None` leaves the owner
    /// polling, which is what the attached TUI's backend does.
    waker: Option<WakeHandle>,
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
            waker: None,
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
            .get(&id)
            .ok_or_else(|| anyhow::anyhow!("pane {id} not found"))?;
        pane.input
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("pane {id} input queue already released"))?
            .enqueue(data)
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
        // What a pass reports, and how a budget spent with events still queued
        // asks to be run again, live in `drain_queued`.
        self.drain_queued()
    }
}

// No explicit `Drop` on `PtyBackend` on purpose: the map drops every pane and
// `PtyPane::drop` handles kill+release+join — an empty Drop would obscure that.

#[cfg(test)]
#[path = "pty_tests.rs"]
mod tests;
