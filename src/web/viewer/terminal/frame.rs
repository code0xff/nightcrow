use crate::backend::PaneId;
use crate::web::viewer::limits;
use serde::{Deserialize, Serialize};

/// A control message from a client. Output travels as binary frames instead,
/// so it never pays JSON escaping or base64 expansion.
///
/// Serialized as well as deserialized: the browser only ever sends these, but
/// an attaching client is Rust on both ends of the same definition, and the
/// daemon relays them.
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
    /// A full desired sequence of the live pane ids, sent when a client drags a
    /// pane to a new slot. The hub reconciles it (see [`super::TerminalHub::reorder_panes`]).
    Reorder {
        order: Vec<PaneId>,
    },
    /// Fill the terminal panel with one pane, or `None` to go back to the grid.
    ///
    /// Which pane is zoomed is the repository's answer rather than each page's,
    /// for the same reason the pane *order* is (see
    /// [`super::TerminalHub::reorder_panes`]): every client shows the same
    /// terminals, and one that laid them out differently from the rest is a
    /// screen nobody asked for. Unlike the order, it is not a request to be
    /// reconciled — the last one to ask wins outright, because there is nothing
    /// to merge.
    Zoom {
        pane: Option<PaneId>,
    },
    /// Take over sizing this repository's panes (see [`ServerMessage::SizeOwner`]).
    ///
    /// A PTY has one size, so one client at a time decides it. Attaching takes
    /// it; this is how a client already attached takes it back, on a keystroke —
    /// deliberately, rather than by the mere act of looking, which would make
    /// glancing at a phone repaint everybody's screen.
    #[serde(rename = "claim_size")]
    ClaimSize,
    /// Give up on whatever recovery is pending for `pane`.
    ///
    /// A person deciding the wait is over outranks the plugin still waiting: the
    /// hold on the pane's slot is dropped and the slot retired, so nothing can
    /// be relaunched into it afterwards. Harmless for a pane with no recovery in
    /// flight — a client can be a beat behind the hold expiring.
    #[serde(rename = "cancel_recovery")]
    CancelRecovery {
        pane: PaneId,
    },
    /// The sizes to give the startup terminals, answering [`ServerMessage::Pending`].
    ///
    /// One entry per pending pane, in the order they will be created. A short
    /// list leaves the rest at the default, so a client that could only measure
    /// some of them still gets terminals.
    Start {
        sizes: Vec<PaneSize>,
    },
    /// How a `Ctrl+L` a client just forwarded came to be — see
    /// [`hub_diag`](super::hub_diag) for why anyone is asking.
    ///
    /// Carries no input, only the provenance of one byte: whether a real key
    /// event produced it, whether that event was the browser's own or something
    /// dispatched at the page, and whether it was a key being held down. Logged
    /// and otherwise ignored; nothing in the hub reads it.
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
    /// `KeyboardEvent.isTrusted`: false means a script dispatched it, which no
    /// keyboard can do.
    pub trusted: bool,
    /// `KeyboardEvent.repeat`: the key is being held down and the OS is
    /// repeating it.
    pub repeat: bool,
    /// `KeyboardEvent.code`, e.g. `KeyL`. Sanitized before it is logged — it
    /// arrives from the page like everything else on this socket.
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

/// A control message to a client.
///
/// Deserialized as well as serialized so the daemon can read one back off a
/// hub session and hand it on to an attached client tagged with its
/// repository — one definition rather than a parallel set that can drift.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        /// Which client asked for this pane, in the id space of the connection
        /// the frame is going out on — a hub client id for the browser, whose
        /// socket *is* the hub session, and the attached client's id for a
        /// daemon relay, which is one hop further out (see the daemon's
        /// `TerminalBridges`). Each recipient compares it against its own id on
        /// that connection.
        ///
        /// `None` means nobody there asked: a pane replayed to a connecting
        /// client, one another client opened, or a startup terminal, which
        /// belongs to the session rather than to whoever sized it. Omitted from
        /// the wire when absent, so the frames the browser already reads are
        /// unchanged.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client: Option<u64>,
        /// What the session calls this pane, when it has a name of its own — a
        /// startup terminal opened under a configured name. Absent for a pane a
        /// client asked for, which that client names itself, and for one nothing
        /// has named: a program emitting OSC 0/2 renames either afterwards.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
    Exited {
        pane: PaneId,
    },
    /// The size a pane's PTY is now set to.
    ///
    /// Broadcast, not answered to whoever asked: a PTY has one size and every
    /// client renders the same grid, so a client that is not the one sizing it
    /// still has to follow — its emulator has to wrap where the child's does.
    Resized {
        pane: PaneId,
        rows: u16,
        cols: u16,
    },
    /// Whether *this* client is the one whose layout sets the pane sizes.
    ///
    /// Addressed rather than broadcast, so the answer needs no identity to
    /// compare against — the hub knows who each client is, and "am I the owner"
    /// is the only thing a client does with it.
    ///
    /// The size follows the most recent arrival (tmux's `window-size latest`),
    /// because that is the client someone is looking at; the others become
    /// spectators until one takes it back with [`ClientMessage::ClaimSize`].
    #[serde(rename = "size_owner")]
    SizeOwner {
        owned: bool,
    },
    Error {
        message: String,
    },
    /// The canonical pane order after a reorder, broadcast to every client so
    /// the sender and any other device converge on the same layout.
    Reordered {
        order: Vec<PaneId>,
    },
    /// Which pane now fills the terminal panel, `None` for none.
    ///
    /// Broadcast like [`Reordered`](Self::Reordered) and replayed to a client on
    /// connect, so a page that reloads comes back zoomed where it left off and
    /// another device follows. `null` is sent rather than omitted: "nothing is
    /// zoomed" is a state a client has to be able to be *told*, not only one it
    /// starts in.
    Zoomed {
        pane: Option<PaneId>,
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
