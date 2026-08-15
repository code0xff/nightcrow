//! Putting a pane's screen in front of a connecting client: which record is
//! replayed, in what order, and how it is framed. The records themselves are
//! defined by [`PaneState`](super::hub_helpers::PaneState).

use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::PaneState;
use crate::backend::PaneId;
use std::sync::mpsc::SyncSender;

/// Back to the normal screen. Sent ahead of a normal-screen replay for a pane
/// whose program is on the alternate one, because the prelude that follows is
/// what switches away from it.
const LEAVE_ALT_SCREEN: &[u8] = b"\x1b[?1049l";

/// Largest payload one replay frame carries.
///
/// **Frame boundaries mean nothing to a client.** It concatenates what arrives
/// into its emulator, whose parser is a state machine that spans writes, so a
/// sequence or a multi-byte character split across two frames is reassembled the
/// same as if it had come in one. Splitting is therefore free.
///
/// A single frame, on the other hand, does have a ceiling: the daemon socket
/// refuses a payload over [`MAX_FRAME_BYTES`](crate::daemon::frame::MAX_FRAME_BYTES)
/// (4 MiB), and unlike the byte ring an alternate-screen pane's screen grows with
/// its area — a large pane covered in per-cell colour, which a truecolour image
/// renderer produces, reaches several megabytes. Sent whole it ended the attach
/// connection, and again on every reconnect, because the same screen was replayed
/// each time. Nobody transmits a screen as one indivisible message: VS Code's
/// replay is a list of entries, tmux writes to a passed file descriptor, and
/// mosh's datagrams cannot hold a screen at all.
///
/// 1 MiB stays well under that ceiling while keeping the frame count low enough
/// that a whole replay of the largest panes this hub allows fits in
/// [`CLIENT_QUEUE_DEPTH`](super::CLIENT_QUEUE_DEPTH) — which is what makes it
/// safe to queue the replay before the client is registered, with nothing else
/// writing to that queue.
pub(super) const REPLAY_CHUNK_BYTES: usize = 1024 * 1024;

/// Queue `data` for `pane` as frames no larger than [`REPLAY_CHUNK_BYTES`],
/// reporting whether all of it fit. Stops at the first frame the queue refuses:
/// what follows would not fit either, and a replay missing a piece in the middle
/// is a screen the client cannot repair.
fn send_replay(tx: &SyncSender<TerminalFrame>, pane: PaneId, data: &[u8]) -> bool {
    data.chunks(REPLAY_CHUNK_BYTES).all(|chunk| {
        tx.try_send(TerminalFrame::Output {
            pane,
            data: chunk.to_vec(),
        })
        .is_ok()
    })
}

/// The bytes that rebuild a pane's normal screen: the ring up to the mark
/// (history — its screen state is superseded by the snapshot, but its scrolled-off
/// lines are still worth scrolling back to), the snapshot itself (an absolute
/// repaint, so whatever viewport the truncated history left behind is erased and
/// the screen arrives whole), then everything recorded since the snapshot was
/// taken. A pane with no snapshot yet has `covered == 0` and an empty
/// `normal_screen`, which makes this the plain ring.
fn normal_record(pane: &PaneState) -> Vec<u8> {
    let mut data = Vec::with_capacity(pane.scrollback.len() + pane.normal_screen.len());
    data.extend(pane.scrollback.iter().copied().take(pane.covered));
    data.extend_from_slice(&pane.normal_screen);
    data.extend(pane.scrollback.iter().copied().skip(pane.covered));
    data
}

/// Announce `pane` to an attaching client and put its screen in front of it: the
/// modes its program established, and then whichever record holds that program's
/// screen (see [`PaneState`](super::hub_helpers::PaneState)).
///
/// Frames go straight onto the client's queue rather than through a broadcast:
/// this runs while the caller holds the state lock, before the client is eligible
/// for broadcasts at all (see [`TerminalHub::connect`](super::TerminalHub::connect)).
///
/// Reports whether the whole pane reached the queue. A client that was handed only
/// part of a screen has no way to tell, so the caller says so where someone can
/// read it.
pub(super) fn replay_pane(tx: &SyncSender<TerminalFrame>, pane: &PaneState) -> bool {
    let mut whole = true;
    if let Ok(json) = serde_json::to_string(&ServerMessage::Created {
        pane: pane.id,
        rows: pane.rows,
        cols: pane.cols,
        title: pane.title.clone(),
        // A replayed pane predates this client, so nobody here asked for it — it
        // must not take the focus of whatever the client is already looking at.
        client: None,
    }) {
        whole &= tx.try_send(TerminalFrame::Control(json)).is_ok();
    }
    // The normal screen first, for a pane whose program has left it: its record
    // was frozen where the program switched away, so it is what the client will be
    // returned to the moment that program exits. Without it, quitting a
    // full-screen program left a client that attached during it looking at a blank
    // screen. Explicitly on the normal buffer, because the prelude below is what
    // switches off it.
    if pane.modes.alt_screen && !(pane.scrollback.is_empty() && pane.normal_screen.is_empty()) {
        let mut data = Vec::with_capacity(LEAVE_ALT_SCREEN.len() + pane.scrollback.len());
        data.extend_from_slice(LEAVE_ALT_SCREEN);
        data.extend(normal_record(pane));
        whole &= send_replay(tx, pane.id, &data);
    }
    // Ahead of the screen: these are the modes the pane's program set once, at
    // startup, and no record of them survives in what follows. Without this a
    // reattaching client is a terminal the program never configured — mouse
    // reporting off, arrows in the wrong encoding, paste unbracketed. It leads
    // with `1049`, so it is also what puts the client on the buffer the program is
    // drawing on before that buffer's contents arrive.
    whole &= send_replay(tx, pane.id, &pane.modes.prelude());
    let data: Vec<u8> = if pane.modes.alt_screen {
        // The screen, then everything broadcast since it was taken — the same
        // bytes every client already attached has seen. (When an entry snapshot
        // was deferred, `since` opens with the switch chunk itself, whose
        // pre-switch text this client plays on the wrong buffer until the next
        // paint covers it — the price of never splicing into an open sequence.)
        let mut data = Vec::with_capacity(pane.screen.len() + pane.since.len());
        data.extend_from_slice(&pane.screen);
        data.extend(pane.since.iter().copied());
        data
    } else {
        normal_record(pane)
    };
    whole &= send_replay(tx, pane.id, &data);
    whole
}
