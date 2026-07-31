use super::frame::{ServerMessage, TerminalFrame};
use crate::backend::PaneId;
use crate::runtime::emulator::PaneModes;
use crate::web::viewer::limits;
use std::collections::VecDeque;
use std::sync::mpsc::{SyncSender, TrySendError};

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
    /// Every startup pane in one command, so *queueing* the set is
    /// all-or-nothing: sending them one by one could spend the claim on some
    /// and lose the rest with nothing left to retry from.
    ///
    /// `reserved` is how many cap slots [`Shared::reserved`] is holding for
    /// this batch, released as the panes take them. Every connection has its
    /// own handler thread, so between the claim and this command reaching the
    /// queue another client can enqueue creates that the worker would serve
    /// first — the reservation is what keeps those from taking slots the
    /// configured set already claimed.
    ///
    /// The set can still come up short of what was configured, because the
    /// reservation only holds what was free at claim time: terminals already
    /// open when the claim happened are not displaced, and any one `openpty`
    /// can still fail. The claim is spent by then, so a command lost that way
    /// does not run until the hub restarts; the client is told which it was
    /// (`terminal limit reached`, `could not start a terminal`). Going further
    /// would mean deciding that a configured command outranks a terminal the
    /// user already opened, which is a question about what the cap means
    /// rather than a race to close.
    CreateStartup {
        panes: Vec<StartupPane>,
        client: u64,
        reserved: usize,
    },
    /// `client` rides along for the arrival log only (see
    /// [`hub_diag`](super::hub_diag)); input is honoured from whoever sends it,
    /// unlike a resize.
    Input {
        pane: PaneId,
        data: Vec<u8>,
        client: u64,
    },
    /// `client` rides along because a resize is only honoured from the client
    /// that owns the sizing (see [`Shared::size_owner`]).
    Resize {
        pane: PaneId,
        rows: u16,
        cols: u16,
        client: u64,
    },
    Close {
        pane: PaneId,
    },
    Reorder {
        order: Vec<PaneId>,
    },
    /// Abandon a pane's pending relaunch. On the worker queue because carrying it
    /// out needs the backend and the plugin bookkeeping, both of which are
    /// worker-local.
    CancelRecovery {
        pane: PaneId,
    },
    /// Make these panes' programs draw their whole screen again, because a
    /// client has just attached and what it was replayed cannot show it (see
    /// [`hub_repaint`](super::hub_repaint)). Queued by `connect`, which holds no
    /// backend of its own.
    Repaint {
        panes: Vec<PaneId>,
    },
    /// Bring this hub's plugin children in line with a re-read `[[plugin]]`
    /// table. On the queue because every plugin host is worker-local — a plugin
    /// can drive a pane's keyboard, so nothing outside the worker may touch one
    /// (see [`hub_reload`](super::hub_reload)).
    ReloadPlugins {
        plugins: Vec<crate::config::PluginConfig>,
    },
}

/// One startup terminal: the command to run, at the size a client measured, under
/// the name it was configured with.
pub struct StartupPane {
    pub(super) size: crate::web::viewer::terminal::frame::PaneSize,
    pub(super) command: Option<String>,
    pub(super) title: Option<String>,
    /// The `[[plugin]]` this pane's configuration handed it to, if any. Carried
    /// only as far as the worker, which records it in a map of its own — it is
    /// deliberately absent from [`PaneState`] and from every `ServerMessage`, so
    /// no client learns which panes a plugin can act on.
    pub(super) plugin: Option<String>,
}

/// A live terminal and the recent raw bytes it has produced, kept so a client
/// that connects (or reconnects after a refresh) can be replayed the current
/// screen. Bounded by [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`].
pub(super) struct PaneState {
    pub(super) id: PaneId,
    /// The name the session gave it, if any — a configured startup terminal has
    /// one before it runs. Kept so a client that connects later is told it too,
    /// rather than showing "shell 1" for a pane every other client calls
    /// something else.
    pub(super) title: Option<String>,
    pub(super) scrollback: VecDeque<u8>,
    /// The size the PTY is currently set to, tracked so a connecting client
    /// learns it and can skip a resize that would change nothing.
    pub(super) rows: u16,
    pub(super) cols: u16,
    /// The terminal state the pane's program has established, kept because the
    /// bytes that established it are not in `scrollback` any more (see
    /// [`PaneModeTracker`](super::hub_modes::PaneModeTracker)). A pane that has
    /// printed nothing yet is a freshly opened terminal.
    pub(super) modes: PaneModes,
}

/// Hub state shared between the worker thread (which mutates panes and
/// broadcasts) and connection threads (which register/unregister clients and
/// snapshot scrollback on connect). Held under one mutex so a connecting
/// client's replay is atomic with the worker's append-and-broadcast: it sees
/// each pane's scrollback exactly once, with no gap before or duplicate of the
/// live output that follows.
pub struct Shared {
    pub(super) clients: Vec<Client>,
    pub(super) panes: Vec<PaneState>,
    /// Cap slots held for startup panes that are claimed but not created yet.
    ///
    /// Counted against the same cap rather than exempt from it, so the ceiling
    /// on real processes per repository stays what it says it is — the
    /// reservation decides *who* gets a slot, never how many there are.
    pub(super) reserved: usize,
    /// The pane filling the panel, when one is (see [`hub_zoom`](super::hub_zoom)).
    ///
    /// Beside `panes` and under the same lock because the two have to agree: a
    /// zoom naming a pane that is not in the list is a client rendering nothing.
    pub(super) zoomed: Option<PaneId>,
}

/// Queue a frame for every client, dropping any that has fallen too far behind.
/// Terminal bytes cannot be skipped, so an overfull client is disconnected
/// rather than served a corrupted stream. Operates on an already-locked client
/// list so the caller can pair it with a pane mutation atomically.
pub(super) fn broadcast_locked(clients: &mut Vec<Client>, frame: TerminalFrame) {
    clients.retain(|client| match client.tx.try_send(frame.clone()) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => {
            tracing::debug!(id = client.id, "viewer: terminal client too slow, dropping");
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    });
}

/// The canonical pane order for a reorder request: the requested ids that are
/// actually live, in the requested order, followed by any live pane the request
/// omitted, in its current order. Unknown requested ids are dropped. The result
/// is always a permutation of `current`, which is what makes a reorder safe
/// against a create or close that raced the request.
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

/// Whether putting a pane in front of a new client was enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Replayed {
    /// The client has everything: the pane's modes and the history that is its
    /// screen.
    Whole,
    /// The modes are restored, but only the program can produce the screen —
    /// see [`hub_repaint`](super::hub_repaint).
    NeedsRepaint,
}

/// Announce `pane` to an attaching client and give it what the pane's record can:
/// the modes its program established, and then its history — unless the program
/// draws on the alternate screen, whose recorded bytes are cell updates against a
/// screen this client does not have. Replaying those paints fragments over a blank
/// screen, which is exactly the mess that makes someone reach for the redraw key.
///
/// Frames go straight onto the client's queue rather than through a broadcast:
/// this runs while the caller holds the state lock, before the client is eligible
/// for broadcasts at all (see [`TerminalHub::connect`](super::TerminalHub::connect)).
pub(super) fn replay_pane(tx: &SyncSender<TerminalFrame>, pane: &PaneState) -> Replayed {
    if let Ok(json) = serde_json::to_string(&ServerMessage::Created {
        pane: pane.id,
        rows: pane.rows,
        cols: pane.cols,
        title: pane.title.clone(),
        // A replayed pane predates this client, so nobody here asked for it — it
        // must not take the focus of whatever the client is already looking at.
        client: None,
    }) {
        let _ = tx.try_send(TerminalFrame::Control(json));
    }
    // Ahead of any history: these are the modes the pane's program set once, at
    // startup, and the history is what no longer contains them. Without this a
    // reattaching client is a terminal the program never configured — mouse
    // reporting off, arrows in the wrong encoding, paste unbracketed.
    let _ = tx.try_send(TerminalFrame::Output {
        pane: pane.id,
        data: pane.modes.prelude(),
    });
    if pane.modes.alt_screen {
        return Replayed::NeedsRepaint;
    }
    if !pane.scrollback.is_empty() {
        let data: Vec<u8> = pane.scrollback.iter().copied().collect();
        let _ = tx.try_send(TerminalFrame::Output {
            pane: pane.id,
            data,
        });
    }
    Replayed::Whole
}

/// Append raw PTY bytes to a pane's scrollback, evicting the oldest bytes to
/// stay within [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`].
pub(super) fn push_scrollback(buf: &mut VecDeque<u8>, data: &[u8]) {
    buf.extend(data.iter().copied());
    if buf.len() > limits::MAX_TERMINAL_SCROLLBACK_BYTES {
        let excess = buf.len() - limits::MAX_TERMINAL_SCROLLBACK_BYTES;
        buf.drain(0..excess);
    }
}
