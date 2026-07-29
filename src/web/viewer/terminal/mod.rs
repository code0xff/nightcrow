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

pub mod frame;
mod hub_helpers;
mod hub_run;
mod session;

#[cfg(test)]
pub use frame::decode_output;
pub use frame::{ClientMessage, PaneSize, ServerMessage, TerminalFrame, encode_output};
pub use session::TerminalSession;

use crate::web::viewer::limits;
use hub_helpers::{Command, Shared, StartupPane};
use session::Client;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

/// Output frames a client may fall behind by before it is dropped.
const CLIENT_QUEUE_DEPTH: usize = 256;

/// The size a pane is born at when no client measured one for it. Only reached
/// when a client answers `Pending` with fewer sizes than there are panes; the
/// first fit corrects it at the cost of one repaint.
const DEFAULT_PANE_SIZE: PaneSize = PaneSize { rows: 24, cols: 80 };

pub struct TerminalHub {
    pub(super) commands: SyncSender<Command>,
    pub(super) state: Mutex<Shared>,
    next_client_id: AtomicU64,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    /// Commands to run in startup terminals, created once a client has sized
    /// them. Empty means a single bare shell (matching the TUI's default).
    startup: Vec<String>,
    /// Set when a client claims the startup terminals by answering with their
    /// sizes, so they are created exactly once for the hub's life rather than
    /// on every (re)connection. See [`TerminalHub::claim_startup`].
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
                reserved: 0,
                size_owner: None,
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
    /// therefore receives every pane's history exactly once and in order
    /// ahead of the live stream. A fresh hub (e.g. after a server restart)
    /// has no panes, so a reconnecting client correctly comes back to an
    /// empty panel.
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
                if let Ok(json) = serde_json::to_string(&ServerMessage::Created {
                    pane: pane.id,
                    rows: pane.rows,
                    cols: pane.cols,
                    // A replayed pane predates this client, so nobody here
                    // asked for it — it must not take the focus of whatever the
                    // client is already looking at.
                    client: None,
                }) {
                    let _ = tx.try_send(TerminalFrame::Control(json));
                }
                if !pane.scrollback.is_empty() {
                    let data: Vec<u8> = pane.scrollback.iter().copied().collect();
                    let _ = tx.try_send(TerminalFrame::Output {
                        pane: pane.id,
                        data,
                    });
                }
            }
        }
        state.clients.push(Client { id, tx });
        // Arriving takes the sizing: this is the client someone just sat down
        // at, and the panes should fit its screen. Taken before the lock is
        // released so two clients connecting at once cannot both end up
        // believing they own it.
        let displaced = state.size_owner.replace(id);
        drop(state);
        self.announce_size_owner(id, displaced);

        // Offer the startup terminals to be sized rather than spawning them
        // here. A PTY created before any client has measured its cell is born
        // at a size nobody chose, and correcting it costs the child a full
        // repaint — so the client answers with `start` and the hub creates
        // them then (see `claim_startup`). Announced to every client while the
        // panes are unclaimed, so one that drops mid-handshake does not leave
        // the hub terminal-less forever.
        if !self.stop.load(Ordering::Acquire) && !self.started.load(Ordering::Acquire) {
            self.send_to(
                id,
                &ServerMessage::Pending {
                    count: self.startup_count(),
                },
            );
        }

        TerminalSession {
            hub: Arc::clone(self),
            id,
            rx: std::sync::Mutex::new(rx),
        }
    }

    /// How many startup terminals this hub will open. No configured commands
    /// means one bare shell, matching the TUI's default.
    fn startup_count(&self) -> usize {
        self.startup.len().max(1)
    }

    /// Create the startup terminals at the sizes a client measured, if nobody
    /// has claimed them yet.
    ///
    /// The claim is what makes this once-per-hub-life. It is taken here rather
    /// than when a client connects because a client that never answers must
    /// not consume it — the next one to connect is offered the panes again,
    /// which is what keeps a dropped handshake from being fatal. Two clients
    /// answering at once is normal (both were offered): the first to arrive
    /// wins the exchange and the second is ignored, so the panes are created
    /// exactly once.
    pub(super) fn claim_startup(&self, client: u64, sizes: &[PaneSize]) {
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let commands: Vec<Option<String>> = if self.startup.is_empty() {
            vec![None]
        } else {
            self.startup.iter().map(|c| Some(c.clone())).collect()
        };
        let panes: Vec<StartupPane> = commands
            .into_iter()
            .enumerate()
            .map(|(index, command)| StartupPane {
                // A client that could only measure some cells still gets the
                // rest; they are simply born at the old default and corrected
                // by the first fit, which is where every pane started before.
                size: sizes.get(index).copied().unwrap_or(DEFAULT_PANE_SIZE),
                command,
            })
            .collect();

        // Hold the free cap slots before the command is even queued. Another
        // connection's handler thread can enqueue creates between here and the
        // worker reaching this batch, and the worker would serve those first;
        // the reservation is what stops them taking slots this set claimed.
        // Only what is free — terminals already open are not displaced.
        let reserved = {
            let mut state = self.state.lock().expect("terminal state poisoned");
            let free = limits::MAX_PTYS_PER_REPO.saturating_sub(state.panes.len() + state.reserved);
            let take = panes.len().min(free);
            state.reserved += take;
            take
        };

        // One command for the whole set, not one per pane. Sent with
        // `try_send` because a full queue means backpressure the connection
        // thread must not block on — and as a single message that is
        // all-or-nothing, so there is no state where some startup terminals
        // were accepted and the rest were silently lost with the claim already
        // spent.
        if self
            .commands
            .try_send(Command::CreateStartup {
                panes,
                client,
                reserved,
            })
            .is_err()
        {
            self.release_reserved(reserved);
            // Hand the claim back and offer again, or the hub would hold
            // `started` with no terminals to show for it — the one outcome
            // this handshake exists to rule out. The re-offer matters as much
            // as the release: this client has already cleared its pending
            // state and will not ask again on its own.
            tracing::warn!("viewer: terminal command queue full, startup deferred");
            self.started.store(false, Ordering::Release);
            // To everyone, not just whoever answered. The offer belongs to
            // whichever client replies first, and the one that just did may be
            // gone by now — or another may have answered while the claim was
            // held and had its `start` ignored, clearing its pending state on
            // the way. Re-offering to only one leaves the rest with no reason
            // to ask again.
            self.broadcast_pending();
        }
    }

    /// Offer the startup terminals to every connected client.
    fn broadcast_pending(&self) {
        let Ok(json) = serde_json::to_string(&ServerMessage::Pending {
            count: self.startup_count(),
        }) else {
            return;
        };
        let mut state = self.state.lock().expect("terminal state poisoned");
        hub_helpers::broadcast_locked(&mut state.clients, TerminalFrame::Control(json));
    }

    /// Give back cap slots a startup batch is no longer going to use.
    pub(super) fn release_reserved(&self, count: usize) {
        if count == 0 {
            return;
        }
        let mut state = self.state.lock().expect("terminal state poisoned");
        state.reserved = state.reserved.saturating_sub(count);
    }

    /// Queue a control message for one client, dropping it if that client has
    /// fallen too far behind.
    fn send_to(&self, client_id: u64, message: &ServerMessage) {
        let Ok(json) = serde_json::to_string(message) else {
            return;
        };
        let mut state = self.state.lock().expect("terminal state poisoned");
        if let Some(index) = state.clients.iter().position(|c| c.id == client_id)
            && state.clients[index]
                .tx
                .try_send(TerminalFrame::Control(json))
                .is_err()
        {
            state.clients.remove(index);
        }
    }

    fn disconnect(&self, id: u64) {
        let heir = {
            let mut state = self.state.lock().expect("terminal state poisoned");
            state.clients.retain(|c| c.id != id);
            if state.size_owner != Some(id) {
                return;
            }
            // The owner left, so the sizing passes to whoever arrived most
            // recently among those still here — the same rule that gave it
            // away in the first place. With nobody left it goes unowned, and
            // every pane keeps the size it has: there is no client to fit.
            state.size_owner = state.clients.last().map(|c| c.id);
            state.size_owner
        };
        if let Some(heir) = heir {
            self.send_to(heir, &ServerMessage::SizeOwner { owned: true });
        }
    }

    /// Move the sizing to `client`, at its own request.
    pub(super) fn claim_size(&self, client: u64) {
        let displaced = {
            let mut state = self.state.lock().expect("terminal state poisoned");
            // A client that has gone cannot take it: its request can arrive
            // after it disconnected, and handing it the sizing would freeze
            // every pane at whatever size it left behind.
            if !state.clients.iter().any(|c| c.id == client) {
                return;
            }
            if state.size_owner == Some(client) {
                return;
            }
            state.size_owner.replace(client)
        };
        self.announce_size_owner(client, displaced);
    }

    /// Tell the new owner it has the sizing, and the one it took it from that it
    /// no longer does.
    fn announce_size_owner(&self, owner: u64, displaced: Option<u64>) {
        self.send_to(owner, &ServerMessage::SizeOwner { owned: true });
        if let Some(displaced) = displaced.filter(|id| *id != owner) {
            self.send_to(displaced, &ServerMessage::SizeOwner { owned: false });
        }
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
            crate::platform::threading::try_timed_join(
                handle,
                crate::platform::threading::REAP_TIMEOUT,
            );
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
