//! Terminals owned by the viewer, one hub per repository.
//!
//! These are **not** the TUI's panes. Sharing them would mean reaching into
//! `App`, which would break both the "server never touches App" rule and the
//! headless mode. The viewer owns its own [`PtyBackend`], so `nightcrow serve`
//! offers terminals with no TUI running at all.
//!
//! Raw PTY bytes go to the browser untouched — the hub renders no screen.
//! xterm.js is a terminal emulator already; the mirror only composes a grid
//! because it has to match a ratatui screen, and this has no such constraint.
//! The hub does parse the stream for one thing it cannot get any other way: the
//! modes each pane's program has set, which an attaching client has to be told
//! because the bytes that set them are long gone (see [`hub_modes`]).
//!
//! **Output is queued, not conflated.** Status updates can drop intermediates
//! because the newest is a complete picture; terminal bytes cannot — dropping
//! any corrupts the stream. Each client gets a bounded queue and is
//! disconnected when it overflows, which is honest, where silently discarding
//! bytes would leave a subtly wrong screen.

pub mod frame;
mod hub_connect;
mod hub_diag;
#[cfg(test)]
mod hub_diag_tests;
mod hub_events;
mod hub_helpers;
mod hub_layout;
mod hub_modes;
mod hub_plugins;
mod hub_plugins_slots;
mod hub_recovery;
mod hub_relaunch;
mod hub_reload;
mod hub_reload_hosts;
mod hub_repaint;
mod hub_run;
mod session;
#[cfg(test)]
mod session_tests;
mod size_owner;
mod startup;
mod startup_run;

#[cfg(test)]
pub use frame::decode_output;
pub use frame::{ClientMessage, PaneSize, TerminalFrame, encode_output};
pub use session::TerminalSession;

use crate::web::viewer::size_owner::SizeOwnership;
use hub_helpers::{Command, Shared};
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
    /// The terminals to open once a client has sized them, with the names they
    /// were configured under. Empty means a single bare shell (matching the
    /// TUI's default).
    startup: Vec<crate::config::StartupCommand>,
    /// The `[[plugin]]` table. The worker launches a host for each entry that is
    /// enabled *and* that some `startup` entry opted into, and nothing else — a
    /// plugin no pane named is never started, so declaring one costs nothing
    /// until a pane hands itself over.
    plugins: Vec<crate::config::PluginConfig>,
    /// Set when a client claims the startup terminals by answering with their
    /// sizes, so they are created exactly once for the hub's life rather than
    /// on every (re)connection. See [`TerminalHub::claim_startup`].
    started: AtomicBool,
    /// Which screen the session's panes are fitted to. The session's rather than
    /// this hub's, because every client shows the same repository — see
    /// [`SizeOwnership`].
    ownership: Arc<SizeOwnership>,
}

impl TerminalHub {
    /// Start a hub whose terminals run in `cwd`. `startup` is the list of
    /// commands to launch when the first client connects (empty = one shell),
    /// and `plugins` the configured plugin table those commands may opt into.
    ///
    /// `ownership` is the session's, shared with every other hub: which screen
    /// the panes are fitted to is one answer for the whole session, not one per
    /// repository.
    pub fn spawn(
        cwd: &str,
        startup: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
        ownership: Arc<SizeOwnership>,
    ) -> Arc<Self> {
        let (commands, command_rx) = mpsc::sync_channel::<Command>(256);
        let hub = Arc::new(Self {
            commands,
            state: Mutex::new(Shared {
                clients: Vec::new(),
                panes: Vec::new(),
                reserved: 0,
            }),
            next_client_id: AtomicU64::new(0),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            startup,
            plugins,
            started: AtomicBool::new(false),
            ownership,
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

    /// The startup panes this hub was spawned with.
    ///
    /// Fixed for its life: the panes are created once and a config reload does
    /// not replace them, so this stays what the repository was opened under.
    /// Read by the plugin reload, which decides what a plugin may see on *this*
    /// hub from the opt-ins this list carries rather than from the new file's.
    pub(crate) fn startup_commands(&self) -> &[crate::config::StartupCommand] {
        &self.startup
    }

    /// How many startup terminals this hub will open. No configured commands
    /// means one bare shell, matching the TUI's default.
    fn startup_count(&self) -> usize {
        self.startup.len().max(1)
    }

    /// Give back cap slots a startup batch is no longer going to use.
    pub(super) fn release_reserved(&self, count: usize) {
        if count == 0 {
            return;
        }
        let mut state = self.state.lock().expect("terminal state poisoned");
        state.reserved = state.reserved.saturating_sub(count);
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
