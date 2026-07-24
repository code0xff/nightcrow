use crate::backend::PaneId;
use super::frame::TerminalFrame;
use crate::web::viewer::limits;
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
    Input { pane: PaneId, data: Vec<u8> },
    Resize { pane: PaneId, rows: u16, cols: u16 },
    Close { pane: PaneId },
    Reorder { order: Vec<PaneId> },
}

/// A live terminal and the recent raw bytes it has produced, kept so a client
/// that connects (or reconnects after a refresh) can be replayed the current
/// screen. Bounded by [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`].
pub(super) struct PaneState {
    pub(super) id: PaneId,
    pub(super) scrollback: VecDeque<u8>,
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

/// Append raw PTY bytes to a pane's scrollback, evicting the oldest bytes to
/// stay within [`limits::MAX_TERMINAL_SCROLLBACK_BYTES`].
pub(super) fn push_scrollback(buf: &mut VecDeque<u8>, data: &[u8]) {
    buf.extend(data.iter().copied());
    if buf.len() > limits::MAX_TERMINAL_SCROLLBACK_BYTES {
        let excess = buf.len() - limits::MAX_TERMINAL_SCROLLBACK_BYTES;
        buf.drain(0..excess);
    }
}