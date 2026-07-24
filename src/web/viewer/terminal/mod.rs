//! Terminals owned by the viewer, one hub per repository.
//!
//! These are **not** the TUI's panes. Sharing them would mean reaching into
//! `App`, which would break both the "server never touches App" rule and the
//! headless mode. The viewer owns its own [`PtyBackend`], so `nightcrow serve`
//! offers terminals with no TUI running at all.
//!
//! Raw PTY bytes go to the browser untouched — no server-side VT emulation.
//! xterm.js is a terminal emulator already; the mirror only composes a grid
//! because it has to match a ratatui screen, and this has no such constraint.
//!
//! **Output is queued, not conflated.** Status updates can drop intermediates
//! because the newest is a complete picture; terminal bytes cannot — dropping
//! any corrupts the stream. Each client gets a bounded queue and is
//! disconnected when it overflows, which is honest, where silently discarding
//! bytes would leave a subtly wrong screen.

mod frame;
mod hub_helpers;
mod hub_run;
mod session;

pub use frame::{
    ClientMessage, ServerMessage, TerminalFrame, encode_output,
};
#[cfg(test)]
pub use frame::decode_output;
pub use session::TerminalSession;

use hub_helpers::{Command, Shared};
use session::Client;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Output frames a client may fall behind by before it is dropped.
const CLIENT_QUEUE_DEPTH: usize = 256;

pub struct TerminalHub {
    pub(super) commands: SyncSender<Command>,
    pub(super) state: Mutex<Shared>,
    next_client_id: AtomicU64,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    /// Commands to run in startup terminals, spawned once when the first client
    /// connects. Empty means a single bare shell (matching the TUI's default).
    startup: Vec<String>,
    /// Set the first time a client connects, so the startup terminals spawn
    /// exactly once for the hub's life rather than on every (re)connection.
    started: AtomicBool,
}

impl TerminalHub {
    /// Start a hub whose terminals run in `cwd`. `startup` is the list of
    /// commands to launch when the first client connects (empty = one shell).
    pub fn spawn(cwd: &str, startup: Vec<String>) -> Arc<Self> {
        let (commands, command_rx) = mpsc::sync_channel::<Command>(256);
        let hub = Arc::new(Self {
            commands,
            state: Mutex::new(Shared {
                clients: Vec::new(),
                panes: Vec::new(),
            }),
            next_client_id: AtomicU64::new(0),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            startup,
            started: AtomicBool::new(false),
        });

        let worker_hub = Arc::clone(&hub);
        let stop = Arc::clone(&hub.stop);
        let cwd = cwd.to_string();
        let handle = thread::Builder::new()
            .name("nightcrow-viewer-term".into())
            .spawn(move || worker_hub.run(&cwd, command_rx, stop))
            .ok();
        *hub.worker.lock().expect("terminal worker slot poisoned") = handle;
        hub
    }

    /// Register a client and replay the current terminals to it before it is
    /// eligible for broadcasts: one `Created` plus one scrollback `Output` per
    /// live pane. Done under the state lock so this snapshot cannot interleave
    /// with the worker's append-and-broadcast (see [`Shared`]); the client
    /// therefore receives every pane's history exactly once and in order ahead
    /// of the live stream. A fresh hub (e.g. after a server restart) has no
    /// panes, so a reconnecting client correctly comes back to an empty panel.
    pub fn connect(self: &Arc<Self>) -> TerminalSession {
        let id = self.next_client_id.fetch_add(1, Ordering::AcqRel);
        let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE_DEPTH);
        let mut state = self.state.lock().expect("terminal state poisoned");
        // A hub whose worker has stopped (its repo was retired) still lingers
        // behind the `Arc` a racing connection resolved, but its panes are dead
        // and will never emit another frame. Skip the replay so the client is
        // not handed phantom terminals it can never receive output or an exit
        // for.
        if !self.stop.load(Ordering::Acquire) {
            for pane in &state.panes {
                if let Ok(json) = serde_json::to_string(&ServerMessage::Created { pane: pane.id }) {
                    let _ = tx.try_send(TerminalFrame::Control(json));
                }
                if !pane.scrollback.is_empty() {
                    let data: Vec<u8> = pane.scrollback.iter().copied().collect();
                    let _ = tx.try_send(TerminalFrame::Output { pane: pane.id, data });
                }
            }
        }
        state.clients.push(Client { id, tx });
        drop(state);

        // First connection spawns the startup terminals (once per hub life):
        // the configured commands, or a single bare shell if none. Queued after
        // the client is registered so it receives the resulting "created"
        // broadcasts, and skipped on a stopped hub.
        if !self.stop.load(Ordering::Acquire)
            && !self.started.swap(true, Ordering::AcqRel)
        {
            if self.startup.is_empty() {
                self.queue_startup_pane(id, None);
            } else {
                for command in &self.startup {
                    self.queue_startup_pane(id, Some(command.clone()));
                }
            }
        }

        TerminalSession {
            hub: Arc::clone(self),
            id,
            rx,
        }
    }

    /// Enqueue a startup terminal. Uses the same command queue as client
    /// creates; a full queue just drops it (the hub is under heavy backpressure,
    /// and a startup pane is not worth wedging the connection thread over).
    fn queue_startup_pane(&self, client: u64, command: Option<String>) {
        let _ = self.commands.try_send(Command::Create {
            rows: 24,
            cols: 80,
            client,
            command,
        });
    }

    fn disconnect(&self, id: u64) {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
            .retain(|c| c.id != id);
    }

    pub fn client_count(&self) -> usize {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
            .len()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
        let handle = self
            .worker
            .lock()
            .expect("terminal worker slot poisoned")
            .take();
        if let Some(handle) = handle {
            crate::util::try_timed_join(handle, crate::util::REAP_TIMEOUT);
        }
    }
}

impl Drop for TerminalHub {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests;