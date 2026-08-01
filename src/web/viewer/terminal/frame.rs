use crate::backend::PaneId;
use crate::web::viewer::limits;
use serde::{Deserialize, Serialize};

/// A control message from a client. Output travels as binary frames to avoid
/// JSON escaping or base64 expansion. Serialized as well as deserialized so the
/// daemon can relay them between Rust clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A client's desired pane order after a drag. The hub reconciles it
    /// (see [`super::TerminalHub::reorder_panes`]).
    Reorder {
        order: Vec<PaneId>,
    },
    /// Fill the terminal panel with one pane, or `None` to go back to the grid.
    /// The last client to ask wins — unlike pane order, there is nothing to merge.
    Zoom {
        pane: Option<PaneId>,
    },
    /// Take over sizing this repository's panes (see [`ServerMessage::SizeOwner`]).
    /// A PTY has one size, so one client at a time decides it. Attaching takes
    /// it; this is how a client already attached takes it back on a keystroke.
    #[serde(rename = "claim_size")]
    ClaimSize,
    /// Give up on whatever recovery is pending for `pane`. The person's decision
    /// outranks the plugin: the hold on the pane's slot is dropped and the slot
    /// retired, so nothing can be relaunched into it afterwards.
    #[serde(rename = "cancel_recovery")]
    CancelRecovery {
        pane: PaneId,
    },
    /// The sizes to give the startup terminals, answering [`ServerMessage::Pending`].
    /// One entry per pending pane, in order. A short list leaves the rest at the
    /// default, so a client that could only measure some still gets terminals.
    Start {
        sizes: Vec<PaneSize>,
    },
    /// How a `Ctrl+L` a client just forwarded came to be — see
    /// [`hub_diag`](super::hub_diag) for why anyone is asking. Carries only the
    /// provenance of one byte. Logged and otherwise ignored.
    #[serde(rename = "clear_key_report")]
    ClearKeyReport {
        pane: PaneId,
        /// `None` when no key event preceded the byte — a paste, an input method,
        /// or a script writing straight into the terminal.
        key: Option<ClearKeyFacts>,
    },
}

/// What the browser said about the key event behind a forwarded `Ctrl+L`.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ClearKeyFacts {
    /// `KeyboardEvent.isTrusted`: false means a script dispatched it.
    pub trusted: bool,
    /// `KeyboardEvent.repeat`: the key is being held down.
    pub repeat: bool,
    /// `KeyboardEvent.code`, e.g. `KeyL`. Sanitized before logging.
    pub code: String,
    /// Milliseconds between that key event and the byte it produced.
    pub since_ms: u32,
}

/// One pane's size, as the client measured its cell.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct PaneSize {
    pub rows: u16,
    pub cols: u16,
}

impl PaneSize {
    /// Bring a size the client sent inside [`limits`]' bounds. Every path that
    /// reaches `openpty` goes through here — `create`, `resize`, and each entry
    /// of `start` — because they all arrive from the same untrusted side.
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

/// A control message to a client. Deserialized as well as serialized so the
/// daemon can read one back off a hub session and relay it to an attached client.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ServerMessage {
    /// A pane exists, along with the size its PTY is currently set to. The size
    /// rides along because the client is not the only source of it: a pane
    /// replayed to a reconnecting page, or one another device sized, already has
    /// a size this client never chose. Without it the client must assume nothing
    /// and send its own size on attach, costing the child a full repaint.
    Created {
        pane: PaneId,
        rows: u16,
        cols: u16,
        /// Which client asked for this pane, in the id space of the connection
        /// the frame is going out on. Each recipient compares it against its own
        /// id on that connection. `None` means nobody there asked: a replayed
        /// pane, one another client opened, or a startup terminal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client: Option<u64>,
        /// What the session calls this pane, when it has a name of its own — a
        /// startup terminal opened under a configured name. Absent for a pane a
        /// client asked for, and for one nothing has named.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Exited {
        pane: PaneId,
    },
    /// The size a pane's PTY is now set to. Broadcast, not answered to whoever
    /// asked: a PTY has one size and every client renders the same grid, so a
    /// client that is not the one sizing it still has to follow.
    Resized {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    /// Who this client is, in the id space [`Created::client`] is stamped in.
    /// Addressed, and the first thing a connection is told. A connection's id,
    /// not a viewer's: minted per connection and a reconnect gets a new one.
    ///
    /// [`Created::client`]: Self::Created::client
    Hello {
        client: u64,
        /// How many `Created` frames the replay is about to deliver. Exact,
        /// because `connect` queues the whole replay under the hub's lock and
        /// only registers the client afterwards. A client that knows the count
        /// can lay its grid out for the panes it is *going* to have rather than
        /// the ones it has so far.
        panes: usize,
    },
    /// Whether *this* client is the one whose layout sets the pane sizes.
    /// Addressed rather than broadcast. The size follows the most recent arrival
    /// (tmux's `window-size latest`); others become spectators until one takes
    /// it back with [`ClientMessage::ClaimSize`].
    #[serde(rename = "size_owner")]
    SizeOwner {
        owned: bool,
    },
    Error {
        message: String,
    },
    /// The canonical pane order after a reorder, broadcast to every client.
    Reordered {
        order: Vec<PaneId>,
    },
    /// Which pane now fills the terminal panel, `None` for none. Broadcast like
    /// [`Reordered`](Self::Reordered) and replayed on connect. `null` is sent
    /// rather than omitted: "nothing is zoomed" is a state a client has to be
    /// told, not only one it starts in.
    Zoomed {
        pane: Option<PaneId>,
    },
    /// This many startup terminals are waiting to be sized before they are
    /// created. The client answers with [`ClientMessage::Start`]. Sent to every
    /// client that connects while they are still unclaimed, so one that drops
    /// mid-handshake does not leave the hub terminal-less forever.
    Pending {
        count: usize,
    },
    /// What a plugin reports about a pane it is nursing back, relayed verbatim.
    ///
    /// Pane metadata rather than screen content: nothing here is drawn into a
    /// terminal grid, and a client that ignores it renders exactly as before.
    /// `state` is the plugin's own short label; the hub neither interprets it nor
    /// keeps it, so this is a broadcast of the latest word and not a state
    /// machine. The one label the hub itself sends is
    /// [`RECOVERY_CANCELLED`](super::hub_recovery::RECOVERY_CANCELLED), which a
    /// client treats as "there is nothing pending any more".
    Recovery {
        pane: PaneId,
        state: String,
        /// A short human line, absent when the plugin gave none. Never carries
        /// transcript or payload text.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        /// When the wait ends, in **unix epoch seconds**, absent when the plugin
        /// is not waiting on a clock. A client renders it in its own local zone;
        /// an absent one must render nothing rather than a guess.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        deadline_epoch: Option<i64>,
        attempt: u32,
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
