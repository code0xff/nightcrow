use super::frame::TerminalFrame;
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::session::limits;
use std::collections::VecDeque;
use std::sync::mpsc::TrySendError;

use super::session::Client;

pub enum Command {
    /// `command` is `Some` only for startup panes (run via `$SHELL -lc`);
    /// client-initiated creates always pass `None` for a bare interactive shell.
    Create {
        rows: u16,
        cols: u16,
        client: u64,
        command: Option<String>,
    },
    /// Every startup pane in one command, so queueing the set is all-or-nothing.
    /// `reserved` is how many cap slots [`Shared::reserved`] is holding for this
    /// batch, released as the panes take them. The reservation keeps other
    /// clients' creates from taking slots the configured set already claimed.
    CreateStartup {
        panes: Vec<StartupPane>,
        client: u64,
        reserved: usize,
    },
    /// `client` rides along for the arrival log only (see
    /// [`hub_diag`](super::hub_diag)); input is honoured from whoever sends it.
    Input {
        pane: PaneId,
        data: Vec<u8>,
        client: u64,
    },
    Close {
        pane: PaneId,
    },
    Reorder {
        order: Vec<PaneId>,
    },
    /// Abandon a pane's pending relaunch. On the worker queue because carrying it
    /// out needs the backend and the plugin bookkeeping, both worker-local.
    CancelRecovery {
        pane: PaneId,
    },
    /// Bring this hub's plugin children in line with a re-read `[[plugin]]`
    /// table. On the queue because every plugin host is worker-local — a plugin
    /// can drive a pane's keyboard, so nothing outside the worker may touch one.
    ReloadPlugins {
        plugins: Vec<crate::config::PluginConfig>,
    },
}

/// The newest size one connection wants for one pane. Resize traffic is kept
/// out of the bounded command queue: intermediate drag positions may collapse,
/// but the final position must remain available to the worker.
pub(super) struct PendingResize {
    pub(super) pane: PaneId,
    pub(super) rows: u16,
    pub(super) cols: u16,
    pub(super) client: u64,
}

/// One startup terminal: the command to run, at the size a client measured, under
/// the name it was configured with.
pub struct StartupPane {
    pub(super) size: crate::session::terminal::frame::PaneSize,
    pub(super) command: Option<String>,
    pub(super) title: Option<String>,
    /// The `[[plugin]]` this pane's configuration handed it to, if any. Carried
    /// only as far as the worker — deliberately absent from [`PaneState`] and
    /// from every `ServerMessage`, so no client learns which panes a plugin can
    /// act on.
    pub(super) plugin: Option<String>,
}

/// A live terminal and what a client that connects has to be given to see it.
///
/// Which record is the pane's screen depends on the mode its program is in, and
/// only one side is written at a time:
///
/// - **Normal screen** — `scrollback`, the raw bytes the pane has produced, with
///   `normal_screen` + `covered` marking a serialized screen partway through
///   them. The ring alone was the record once, but it is byte-bounded and a
///   program that repaints in place — a prompt box, a spinner, a status line —
///   rotates it without ever scrolling: after a long idle the bytes that painted
///   the top of the screen had been evicted, and a replay rebuilt only the
///   repeatedly-redrawn bottom. So replay is `scrollback[..covered]` (history),
///   then `normal_screen` (the screen as of that point, an absolute repaint),
///   then `scrollback[covered..]` — the front of the ring may be evicted freely
///   and the screen still arrives whole (see
///   [`replay_pane`](super::hub_replay::replay_pane)).
/// - **Alternate screen** — `screen` + `since`. The raw bytes are cell updates
///   against a screen a new client does not have, so what is kept instead is the
///   screen itself, serialized (`hub_modes::PaneModeTracker::snapshot`). While a
///   program is on the alternate screen the normal-screen record is left frozen,
///   holding the screen it will be returned to.
pub(super) struct PaneState {
    pub(super) id: PaneId,
    /// What this pane goes by: the name the session gave a configured startup
    /// terminal, and then whatever its program has titled itself since (OSC 0/2,
    /// followed in [`hub_modes`](super::hub_modes)). Kept so a client that
    /// connects later is told it too — the bytes that set it leave `scrollback`
    /// within seconds, so nothing else could tell that client.
    pub(super) title: Option<String>,
    pub(super) scrollback: VecDeque<u8>,
    /// The pane's normal screen as of `covered` bytes into `scrollback`,
    /// serialized the way `screen` is. Empty until the worker first takes one —
    /// a ring that has never evicted rebuilds the screen on its own.
    pub(super) normal_screen: Vec<u8>,
    /// How many bytes at the front of `scrollback` `normal_screen` accounts for.
    /// Only those may be evicted: they are history whose effect on the screen the
    /// snapshot already carries. The bytes past the mark are what a replay
    /// applies *on top of* the snapshot, and dropping any of them would hand a
    /// connecting client a screen missing an update nothing would ever repair.
    pub(super) covered: usize,
    /// This pane's screen as of the last snapshot, empty unless its program is on
    /// the alternate screen.
    pub(super) screen: Vec<u8>,
    /// Bytes broadcast since `screen` was taken. A snapshot is refreshed once per
    /// worker tick, so a client can connect between the broadcast of a chunk and
    /// the refresh that accounts for it; replaying `screen` then `since` is what
    /// makes the two add up to exactly what every other client has seen.
    ///
    /// **Never dropped, only superseded.** Terminal bytes cannot be skipped, so
    /// outgrowing [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`] forces a fresh
    /// snapshot (which empties this) rather than evicting from the front.
    pub(super) since: VecDeque<u8>,
    /// The size the PTY is currently set to, tracked so a connecting client
    /// learns it and can skip a resize that would change nothing.
    pub(super) rows: u16,
    pub(super) cols: u16,
    /// The terminal state the pane's program has established, kept because the
    /// bytes that established it are not in `scrollback` any more (see
    /// [`PaneModeTracker`](super::hub_modes::PaneModeTracker)).
    pub(super) modes: PaneModes,
}

/// Hub state shared between the worker thread (which mutates panes and
/// broadcasts) and connection threads (which register/unregister clients and
/// snapshot scrollback on connect). Held under one mutex so a connecting
/// client's replay is atomic with the worker's append-and-broadcast.
pub struct Shared {
    pub(super) clients: Vec<Client>,
    pub(super) panes: Vec<PaneState>,
    /// Cap slots held for startup panes that are claimed but not created yet.
    /// Counted against the same cap rather than exempt from it, so the ceiling
    /// on real processes per repository stays what it says it is.
    pub(super) reserved: usize,
    /// The pane filling the panel, when one is (see [`hub_zoom`](super::hub_zoom)).
    /// Beside `panes` and under the same lock because the two have to agree.
    pub(super) zoomed: Option<PaneId>,
}

/// Queue a frame for every client, dropping any that has fallen too far behind.
/// Terminal bytes cannot be skipped, so an overfull client is disconnected
/// outright (see [`Client::cut_off`]). Operates on an already-locked client list
/// so the caller can pair it with a pane mutation atomically.
pub(super) fn broadcast_locked(clients: &mut Vec<Client>, frame: TerminalFrame) {
    clients.retain(|client| match client.tx.try_send(frame.clone()) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            // At WARN, not DEBUG: this is the one place a client is disconnected
            // against its will, and it answers by rebuilding every pane from the
            // replay. A person watches that happen, so the default log level has
            // to be able to say why it did.
            tracing::warn!(id = client.id, "viewer: terminal client too slow, dropping");
            client.cut_off();
            false
        }
        // Already gone: its session dropped, which is what closed the receiver.
        Err(TrySendError::Disconnected(_)) => false,
    });
}

/// The canonical pane order for a reorder request: the requested ids that are
/// actually live, in the requested order, followed by any live pane the request
/// omitted, in its current order. The result is always a permutation of
/// `current`, which is what makes a reorder safe against a create or close that
/// raced the request.
pub(super) fn canonical_order(current: &[PaneId], requested: &[PaneId]) -> Vec<PaneId> {
    let mut out: Vec<PaneId> = Vec::with_capacity(current.len());
    // Requested ids first: live, and each taken once (a repeated id would make
    // the result a non-permutation of `current`).
    for id in requested {
        if current.contains(id) && !out.contains(id) {
            out.push(*id);
        }
    }
    // Then any live pane the request left out, in its current order.
    for id in current {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

/// Append raw PTY bytes to a pane's scrollback, evicting the oldest bytes to
/// stay within [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`] — but never past the
/// `covered` mark, whose tail a replay cannot do without (see
/// [`PaneState::covered`]). `covered` moves back with the bytes it counts.
///
/// Reports the uncovered tail's length. Past the cap, only a fresh snapshot
/// that moves the mark can bring the ring back under it: terminal bytes cannot
/// be skipped, so until then the ring runs over rather than dropping any — the
/// same rule [`PaneState::since`] lives by. The length rather than a flag,
/// because the caller weighs *how far* over (see the worker's crowded and
/// desperate thresholds in [`hub_run`](super::TerminalHub::run)).
pub(super) fn push_scrollback(buf: &mut VecDeque<u8>, covered: &mut usize, data: &[u8]) -> usize {
    buf.extend(data.iter().copied());
    if buf.len() > limits::MAX_TERMINAL_SCROLLBACK_BYTES {
        let excess = buf.len() - limits::MAX_TERMINAL_SCROLLBACK_BYTES;
        let evicted = excess.min(*covered);
        buf.drain(0..evicted);
        *covered -= evicted;
    }
    buf.len() - *covered
}
