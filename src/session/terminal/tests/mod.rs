//! The hub's test harness: the shared deadline and the frame accessors every
//! hub test reads its assertions through. The tests themselves live beside it.

mod attention;
mod backpressure;
mod behavior;
mod broadcast;
mod frames;
mod identity;
mod modes;
mod normal_records;
mod plugin_reload;
mod plugin_reload_panes;
mod plugin_rules;
mod plugin_slots;
mod plugin_watch;
mod plugins;
mod pty_fixture;
mod reattach;
mod recovery;
mod recovery_replay;
mod replay_budget;
mod replay_complete;
mod replay_delivery;
mod replay_fidelity;
mod replay_locking;
mod replay_record;
mod replay_registration;
mod replay_window;
mod ring;
mod screen_records;
mod screen_replay;
mod scrollback_depth;
mod size_owner;
mod size_owner_resize_race;
mod snapshot_cycles;
mod snapshot_desk;
mod snapshot_pace;
mod snapshot_pty_windows;
mod snapshot_resize_pace;
mod snapshot_throttle;
mod snapshot_trailing;
mod startup;
mod startup_policy;
mod waiting;
mod wakeup;
mod wakeup_deadline;
mod wire;
mod zoom;

use crate::backend::PaneId;
use crate::session::terminal::{ConcurrencyTestPoint, TerminalHub};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use super::TerminalSession;

use frames::{
    created_pane, created_requester, created_size, created_title, exited_pane, hello_client,
    hello_panes, pending_count, reordered_order, resized_size, zoomed_pane,
};

pub(super) use waiting::{SHELL_TEST_DEADLINE, next_matching, wait_for};

/// How long a test waits for a connection to reach a point a hook announces.
pub(super) const HOOK_DEADLINE: Duration = Duration::from_secs(5);

/// How long a hub may take to notice work once its worker is parked. Well under
/// the one-second cap the worker otherwise waits on, so a missing signal cannot
/// pass for promptness.
pub(super) const PROMPT: Duration = Duration::from_millis(500);

/// Wait until the worker is parked on its wake signal: its pass count has stopped
/// moving for longer than a pass takes.
pub(super) fn wait_until_parked(hub: &Arc<TerminalHub>) {
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    let mut last = hub.worker_passes();
    let mut quiet_since = Instant::now();
    while Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
        let passes = hub.worker_passes();
        if passes != last {
            last = passes;
            quiet_since = Instant::now();
        } else if quiet_since.elapsed() >= Duration::from_millis(50) {
            return;
        }
    }
    panic!("the worker never parked");
}

/// Hold the next connection at the replay boundary: the receiver fires once the
/// hub's state lock is free again and the replay is not framed yet, and sending on
/// the returned sender lets it continue.
///
/// The gate is one-shot: a test that connects anything else afterwards installs a
/// no-op hook first.
pub(super) fn pause_at_framing(hub: &Arc<TerminalHub>) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (framing, reached) = mpsc::channel();
    let (resume, wait) = mpsc::channel();
    let wait = Mutex::new(wait);
    hub.set_concurrency_test_hook(move |point| {
        if point == ConcurrencyTestPoint::ReplayFraming {
            framing.send(()).ok();
            wait.lock().expect("resume gate poisoned").recv().ok();
        }
    });
    (reached, resume)
}

/// Collect the ids of the first `n` distinct panes announced to `session`,
/// in the order the `created` frames arrive.
pub(super) fn collect_created(session: &TerminalSession, n: usize) -> Vec<PaneId> {
    let mut ids = Vec::new();
    while ids.len() < n {
        let created =
            next_matching(session, |f| created_pane(f).is_some()).expect("no created message");
        let id = created_pane(&created).unwrap();
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// A hub with a size ownership of its own.
///
/// Every test but the size-ownership ones wants an isolated session, so this is
/// the default: two hubs from two calls share nothing, exactly as two separate
/// sessions would. A test that needs one session across several repositories
/// builds the `SizeOwnership` itself and calls `TerminalHub::spawn`.
pub(super) fn spawn_hub(
    cwd: &str,
    startup: Vec<crate::config::StartupCommand>,
    plugins: Vec<crate::config::PluginConfig>,
) -> std::sync::Arc<super::TerminalHub> {
    spawn_hub_with_auto_open(cwd, startup, plugins, true)
}

pub(super) fn spawn_hub_with_auto_open(
    cwd: &str,
    startup: Vec<crate::config::StartupCommand>,
    plugins: Vec<crate::config::PluginConfig>,
    auto_open: bool,
) -> std::sync::Arc<super::TerminalHub> {
    super::TerminalHub::spawn(
        cwd,
        startup,
        plugins,
        auto_open,
        crate::config::ShellConfig::default(),
        Default::default(),
    )
}

/// A client arriving at `hub` — a page someone just opened, which is what every
/// test that is not about ownership means by connecting.
pub(super) fn attach(hub: &std::sync::Arc<super::TerminalHub>) -> TerminalSession {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let id = crate::session::size_owner::ViewerId::Browser(format!(
        "test-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    hub.connect(id, true, None)
}

/// A client arriving over a real socket, with the peer end returned so a test
/// can watch what the hub does to the connection itself.
///
/// The served end is returned too, and a test must hold it: the hub is given a
/// *clone*, exactly as `serve_terminal` gives it one while the connection thread
/// keeps the original. Letting the hub's clone be the only handle would close
/// the connection when the client record drops, which is not what production
/// does and would make a test pass without the shutdown it means to check.
pub(super) fn attach_over_socket(
    hub: &std::sync::Arc<super::TerminalHub>,
) -> (TerminalSession, std::net::TcpStream, std::net::TcpStream) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("no loopback listener");
    let addr = listener.local_addr().expect("no local address");
    let peer = std::net::TcpStream::connect(addr).expect("could not dial the listener");
    let (served, _) = listener.accept().expect("no inbound connection");
    let id = crate::session::size_owner::ViewerId::Browser(format!(
        "socket-{}",
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let handed = served.try_clone().expect("could not clone the served end");
    (hub.connect(id, true, Some(handed)), peer, served)
}
