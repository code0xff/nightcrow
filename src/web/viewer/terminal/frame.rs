use crate::backend::PaneId;
use crate::web::viewer::limits;
use serde::{Deserialize, Serialize};

/// A control message from the browser. Output travels as binary frames
/// instead, so it never pays JSON escaping or base64 expansion.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ClientMessage {
    Create {
        rows: u16,
        cols: u16,
    },
    Input {
        pane: PaneId,
        data: String,
    },
    Resize {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    Close {
        pane: PaneId,
    },
    /// A full desired sequence of the live pane ids, sent when a client drags a
    /// pane to a new slot. The hub reconciles it (see [`super::TerminalHub::reorder_panes`]).
    Reorder {
        order: Vec<PaneId>,
    },
    /// The sizes to give the startup terminals, answering [`ServerMessage::Pending`].
    ///
    /// One entry per pending pane, in the order they will be created. A short
    /// list leaves the rest at the default, so a client that could only measure
    /// some of them still gets terminals.
    Start {
        sizes: Vec<PaneSize>,
    },
}

/// One pane's size, as the client measured its cell.
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct PaneSize {
    pub rows: u16,
    pub cols: u16,
}

impl PaneSize {
    /// Bring a size the client sent inside [`limits`]' bounds.
    ///
    /// Every path that reaches `openpty` goes through here — `create`,
    /// `resize`, and each entry of `start` — because they are all the same
    /// thing arriving from the same untrusted side, and a clamp that only some
    /// of them apply is the one the next path forgets.
    pub fn clamped(self) -> Self {
        Self {
            rows: self
                .rows
                .clamp(limits::MIN_PANE_DIMENSION, limits::MAX_PANE_ROWS),
            cols: self
                .cols
                .clamp(limits::MIN_PANE_DIMENSION, limits::MAX_PANE_COLS),
        }
    }
}

/// A control message to the browser.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerMessage {
    /// A pane exists, along with the size its PTY is currently set to.
    ///
    /// The size rides along because the client is not the only source of it:
    /// a pane replayed to a reconnecting page, or one another device sized,
    /// already has a size this client never chose. Without it the client must
    /// assume nothing and send its own size on attach, and every such resize
    /// costs the child a full repaint — even when the two agree.
    Created {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    Exited {
        pane: PaneId,
    },
    Error {
        message: String,
    },
    /// The canonical pane order after a reorder, broadcast to every client so
    /// the sender and any other device converge on the same layout.
    Reordered {
        order: Vec<PaneId>,
    },
    /// This many startup terminals are waiting to be sized before they are
    /// created. The client answers with [`ClientMessage::Start`].
    ///
    /// Sent to every client that connects while they are still unclaimed, not
    /// only the first: a client that disconnects mid-handshake would otherwise
    /// leave the hub with no terminals and no way to ever get them.
    Pending {
        count: usize,
    },
}

/// One frame queued for a connected client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalFrame {
    /// Raw PTY bytes for `pane`. Sent as a binary WebSocket frame with the
    /// pane id prefixed, so one socket multiplexes every terminal losslessly.
    Output { pane: PaneId, data: Vec<u8> },
    /// A JSON control frame.
    Control(String),
}

/// Encode an output frame: 4-byte little-endian pane id, then the raw bytes.
///
/// Binary rather than JSON because PTY output is not guaranteed valid UTF-8 —
/// a multi-byte sequence is routinely split across reads, and lossy decoding
/// would corrupt it before xterm.js ever reassembles it.
pub fn encode_output(pane: PaneId, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 4);
    out.extend_from_slice(&pane.to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// Decode an output frame produced by [`encode_output`].
pub fn decode_output(frame: &[u8]) -> Option<(PaneId, &[u8])> {
    if frame.len() < 4 {
        return None;
    }
    let (id_bytes, rest) = frame.split_at(4);
    let pane = PaneId::from_le_bytes(id_bytes.try_into().ok()?);
    Some((pane, rest))
}
