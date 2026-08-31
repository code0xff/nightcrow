//! Terminals owned by the viewer, one hub per repository.
//!
//! These are **not** the TUI's panes — the viewer owns its own [`PtyBackend`],
//! so `nightcrow -d` offers terminals with no TUI running at all.
//!
//! Raw PTY bytes go to the browser untouched. The hub does parse the stream —
//! through a per-pane emulator — for what it cannot get any other way: the
//! modes each pane's program has set, which an attaching client has to be told
//! because the bytes that set them are long gone (see [`hub_modes`]), and the
//! screen itself wherever the recorded bytes cannot rebuild it (see
//! [`hub_replay`] and [`PaneState`](hub_helpers::PaneState)).
//!
//! **Output is queued, not conflated.** Status updates can drop intermediates
//! because the newest is a complete picture; terminal bytes cannot — dropping
//! any corrupts the stream. Each client gets a bounded queue and is
//! disconnected when it overflows.

pub mod frame;
mod hub_connect;
mod hub_diag;
#[cfg(test)]
mod hub_diag_tests;
mod hub_events;
mod hub_helpers;
mod hub_layout;
mod hub_modes;
mod hub_panes;
mod hub_plugins;
mod hub_plugins_slots;
mod hub_recovery;
mod hub_relaunch;
mod hub_reload;
mod hub_reload_hosts;
mod hub_replay;
mod hub_run;
mod hub_zoom;
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

use crate::session::size_owner::SizeOwnership;
use hub_helpers::{Command, PendingResize, Shared};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConcurrencyTestPoint {
    BeforeResizeValidation,
    DisconnectStateAcquired,
    DisconnectStateContended,
}

#[cfg(test)]
type ConcurrencyTestHook = Arc<dyn Fn(ConcurrencyTestPoint) + Send + Sync>;

/// Output frames a client may fall behind by before it is dropped.
pub(crate) const CLIENT_QUEUE_DEPTH: usize = 256;

/// Default pane size when no client measured one. Only reached when a client
/// answers `Pending` with fewer sizes than there are panes; the first fit
/// corrects it at the cost of one repaint.
const DEFAULT_PANE_SIZE: PaneSize = PaneSize { rows: 24, cols: 80 };

pub struct TerminalHub {
    pub(super) commands: SyncSender<Command>,
    /// Latest resize per connection and pane. Separate from `commands` so a
    /// full input queue cannot discard the final width of a window drag.
    pending_resizes: Mutex<BTreeMap<(u64, crate::backend::PaneId), PendingResize>>,
    pub(super) state: Mutex<Shared>,
    next_client_id: AtomicU64,
    stop: Arc<AtomicBool>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    /// The configured terminals to open once a client has sized them.
    startup: Vec<crate::config::StartupCommand>,
    /// Whether an empty startup list still opens one bare shell.
    auto_open: bool,
    /// The `[[plugin]]` table. The worker launches a host for each entry that is
    /// enabled *and* that some `startup` entry opted into — a plugin no pane
    /// named is never started.
    plugins: Vec<crate::config::PluginConfig>,
    /// The shell every terminal pane is spawned with.
    shell: crate::config::ShellConfig,
    /// Set when a client claims the startup terminals by answering with their
    /// sizes, so they are created exactly once for the hub's life.
    started: AtomicBool,
    /// Which screen the session's panes are fitted to — shared with every other
    /// hub because the answer is one per session, not one per repository.
    ownership: Arc<SizeOwnership>,
    #[cfg(test)]
    concurrency_test_hook: Mutex<Option<ConcurrencyTestHook>>,
}

impl TerminalHub {
    /// Start a hub whose terminals run in `cwd`. `startup` is the list of
    /// commands to launch when the first client connects. `auto_open` adds one
    /// bare shell only when that list is empty, and `plugins` is the configured
    /// plugin table those commands may opt into.
    ///
    /// `ownership` is the session's, shared with every other hub.
    pub fn spawn(
        cwd: &str,
        startup: Vec<crate::config::StartupCommand>,
        plugins: Vec<crate::config::PluginConfig>,
        auto_open: bool,
        shell: crate::config::ShellConfig,
        ownership: Arc<SizeOwnership>,
    ) -> Arc<Self> {
        let (commands, command_rx) = mpsc::sync_channel::<Command>(256);
        let hub = Arc::new(Self {
            commands,
            pending_resizes: Mutex::new(BTreeMap::new()),
            state: Mutex::new(Shared {
                clients: Vec::new(),
                panes: Vec::new(),
                reserved: 0,
                zoomed: None,
            }),
            next_client_id: AtomicU64::new(0),
            stop: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            startup,
            auto_open,
            plugins,
            shell,
            started: AtomicBool::new(false),
            ownership,
            #[cfg(test)]
            concurrency_test_hook: Mutex::new(None),
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

    /// The startup panes this hub was spawned with. Fixed for its life: the
    /// panes are created once and a config reload does not replace them.
    pub(crate) fn startup_commands(&self) -> &[crate::config::StartupCommand] {
        &self.startup
    }

    #[cfg(test)]
    pub(crate) fn auto_opens_shell(&self) -> bool {
        self.auto_open
    }

    /// Pane identities owned by this repository's hub, in canonical order.
    pub(crate) fn pane_ids(&self) -> Vec<crate::backend::PaneId> {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .panes
            .iter()
            .map(|pane| pane.id)
            .collect()
    }

    /// How many startup terminals this hub will open.
    fn startup_count(&self) -> usize {
        if self.startup.is_empty() && self.auto_open {
            1
        } else {
            self.startup.len()
        }
    }

    /// Give back cap slots a startup batch is no longer going to use.
    pub(super) fn release_reserved(&self, count: usize) {
        if count == 0 {
            return;
        }
        let mut state = self.state.lock().expect("terminal state poisoned");
        state.reserved = state.reserved.saturating_sub(count);
    }

    #[cfg(test)]
    pub fn client_count(&self) -> usize {
        self.state
            .lock()
            .expect("terminal state poisoned")
            .clients
            .len()
    }

    #[cfg(test)]
    pub(super) fn set_concurrency_test_hook(
        &self,
        hook: impl Fn(ConcurrencyTestPoint) + Send + Sync + 'static,
    ) {
        *self
            .concurrency_test_hook
            .lock()
            .expect("terminal concurrency test hook poisoned") = Some(Arc::new(hook));
    }

    #[cfg(test)]
    pub(super) fn run_concurrency_test_hook(&self, point: ConcurrencyTestPoint) {
        let hook = self
            .concurrency_test_hook
            .lock()
            .expect("terminal concurrency test hook poisoned")
            .clone();
        if let Some(hook) = hook {
            hook(point);
        }
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
