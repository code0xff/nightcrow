//! Putting a pane's screen in front of a connecting client: which record is
//! replayed, in what order, and how it is framed. The records themselves are
//! defined by [`PaneState`](super::hub_helpers::PaneState).

use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::PaneState;
use crate::backend::PaneId;
use std::collections::VecDeque;
use std::ops::Range;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

#[cfg(test)]
#[path = "hub_replay_bench_tests.rs"]
mod bench;

/// Initial replay frames are bounded independently from the one frame that
/// announces the replay boundary. `connect` reserves that final queue slot.
pub(super) struct ReplayQueue<'a> {
    tx: &'a SyncSender<TerminalFrame>,
    queued: usize,
}

impl<'a> ReplayQueue<'a> {
    pub(super) fn new(tx: &'a SyncSender<TerminalFrame>) -> Self {
        Self { tx, queued: 0 }
    }

    pub(super) fn send(&mut self, frame: TerminalFrame) -> bool {
        if self.queued >= super::CLIENT_QUEUE_DEPTH {
            return false;
        }
        if self.tx.try_send(frame).is_err() {
            return false;
        }
        self.queued += 1;
        true
    }
}

/// Back to the normal screen. Sent ahead of a normal-screen replay for a pane
/// whose program is on the alternate one, because the prelude that follows is
/// what switches away from it.
const LEAVE_ALT_SCREEN: &[u8] = b"\x1b[?1049l";

/// Largest payload one replay frame carries.
///
/// Frame boundaries mean nothing to a client (it concatenates into its
/// emulator, whose parser spans writes), so splitting is free — but a single
/// frame has a ceiling: the daemon socket refuses a payload over
/// [`MAX_FRAME_BYTES`](crate::daemon::frame::MAX_FRAME_BYTES) (4 MiB), and an
/// alternate-screen pane's screen grows with its area, so sending one whole
/// ended the attach connection on every reconnect. 1 MiB stays well under that
/// ceiling while keeping the frame count low enough that a whole replay of the
/// largest panes this hub allows fits in
/// [`CLIENT_QUEUE_DEPTH`](super::CLIENT_QUEUE_DEPTH) — which is what lets the
/// whole replay be queued in one go, with the client's live frames held behind
/// it. The completion marker has one additional reserved slot in the channel.
pub(super) const REPLAY_CHUNK_BYTES: usize = 1024 * 1024;

/// Queue `data` for `pane` as frames no larger than [`REPLAY_CHUNK_BYTES`],
/// reporting whether all of it fit. Stops at the first frame the queue refuses:
/// what follows would not fit either, and a replay missing a piece in the middle
/// is a screen the client cannot repair.
fn send_replay(queue: &mut ReplayQueue<'_>, pane: PaneId, data: &[u8]) -> bool {
    data.chunks(REPLAY_CHUNK_BYTES).all(|chunk| {
        queue.send(TerminalFrame::Output {
            pane,
            data: Arc::from(chunk),
        })
    })
}

/// The bytes that rebuild a pane's normal screen: the ring up to the mark
/// (history — its screen state is superseded by the snapshot, but its scrolled-off
/// lines are still worth scrolling back to), the snapshot itself (an absolute
/// repaint, so whatever viewport the truncated history left behind is erased and
/// the screen arrives whole), then everything recorded since the snapshot was
/// taken. A pane with no snapshot yet has `covered == 0` and an empty
/// `normal_screen`, which makes this the plain ring.
fn push_normal_record(data: &mut Vec<u8>, pane: &PaneState) {
    push_ring(data, &pane.scrollback, 0..pane.covered);
    data.extend_from_slice(&pane.normal_screen);
    push_ring(data, &pane.scrollback, pane.covered..pane.scrollback.len());
}

fn normal_record(pane: &PaneState) -> Vec<u8> {
    let mut data = Vec::with_capacity(pane.scrollback.len() + pane.normal_screen.len());
    push_normal_record(&mut data, pane);
    data
}

/// Append `range` of a deque's bytes, in one copy per contiguous slice.
fn push_ring(data: &mut Vec<u8>, ring: &VecDeque<u8>, range: Range<usize>) {
    let (front, back) = ring.as_slices();
    let start = range.start.min(ring.len());
    let end = range.end.min(ring.len()).max(start);
    if start < front.len() {
        data.extend_from_slice(&front[start..end.min(front.len())]);
    }
    if end > front.len() {
        let skipped = start.saturating_sub(front.len());
        data.extend_from_slice(&back[skipped..end - front.len()]);
    }
}

/// One pane's replay, captured under the hub's state lock so the copies are made
/// once and the framing can happen after the lock is released.
pub(super) struct PaneReplay {
    pub(super) pane: PaneId,
    rows: u16,
    cols: u16,
    title: Option<String>,
    /// The normal record to put on the normal buffer first, for a pane whose
    /// program has left it (see [`replay_pane`]).
    normal_under: Option<Vec<u8>>,
    prelude: Vec<u8>,
    record: Vec<u8>,
}

impl PaneReplay {
    pub(super) fn capture(pane: &PaneState) -> Self {
        let normal_under = (pane.modes.alt_screen
            && !(pane.scrollback.is_empty() && pane.normal_screen.is_empty()))
        .then(|| {
            let mut data = Vec::with_capacity(
                LEAVE_ALT_SCREEN.len() + pane.scrollback.len() + pane.normal_screen.len(),
            );
            data.extend_from_slice(LEAVE_ALT_SCREEN);
            push_normal_record(&mut data, pane);
            data
        });
        let record = if pane.modes.alt_screen {
            let mut data = Vec::with_capacity(pane.screen.len() + pane.since.len());
            data.extend_from_slice(&pane.screen);
            push_ring(&mut data, &pane.since, 0..pane.since.len());
            data
        } else {
            normal_record(pane)
        };
        Self {
            pane: pane.id,
            rows: pane.rows,
            cols: pane.cols,
            title: pane.title.clone(),
            normal_under,
            prelude: pane.modes.prelude(),
            record,
        }
    }
}

/// Announce a captured pane to an attaching client and put its screen in front
/// of it: the modes its program established, and then whichever record holds that
/// program's screen (see [`PaneState`](super::hub_helpers::PaneState)).
///
/// Runs after [`TerminalHub::connect`](super::TerminalHub::connect) has released
/// the hub state lock; the client's replay gate keeps live frames behind these.
///
/// Reports whether the whole pane reached the queue. A client that was handed only
/// part of a screen has no way to tell, so the caller says so where someone can
/// read it.
pub(super) fn replay_pane(queue: &mut ReplayQueue<'_>, replay: &PaneReplay) -> bool {
    let mut whole = true;
    if let Ok(json) = serde_json::to_string(&ServerMessage::Created {
        pane: replay.pane,
        rows: replay.rows,
        cols: replay.cols,
        title: replay.title.clone(),
        // A replayed pane predates this client, so nobody here asked for it — it
        // must not take the focus of whatever the client is already looking at.
        client: None,
    }) {
        whole &= queue.send(TerminalFrame::Control(json));
    }
    // The normal screen first, for a pane whose program has left it: its record
    // was frozen where the program switched away, so it is what the client will be
    // returned to the moment that program exits. Without it, quitting a
    // full-screen program left a client that attached during it looking at a blank
    // screen. Explicitly on the normal buffer, because the prelude below is what
    // switches off it.
    if let Some(data) = &replay.normal_under {
        whole &= send_replay(queue, replay.pane, data);
    }
    // Ahead of the screen: these are the modes the pane's program set once, at
    // startup, and no record of them survives in what follows. Without this a
    // reattaching client is a terminal the program never configured — mouse
    // reporting off, arrows in the wrong encoding, paste unbracketed. It
    // leads with `1049`, so it is also what puts the client on the buffer the
    // program is drawing on before that buffer's contents arrive.
    whole &= send_replay(queue, replay.pane, &replay.prelude);
    // For an alternate-screen pane the record is its screen, then everything
    // broadcast since it was taken — the same bytes every client already attached
    // has seen. (When an entry snapshot was deferred, `since` opens with the
    // switch chunk itself, whose pre-switch text plays on the wrong buffer until
    // the next paint covers it — the price of never splicing into an open
    // sequence.)
    whole &= send_replay(queue, replay.pane, &replay.record);
    whole
}
