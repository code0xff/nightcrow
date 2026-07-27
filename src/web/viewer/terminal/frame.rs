use crate::backend::PaneId;
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
