//! Putting a pane's screen in front of a connecting client: which record is
//! replayed, in what order, and how it is framed. The records themselves are
//! defined by [`PaneState`](super::hub_helpers::PaneState).

use super::frame::{ServerMessage, TerminalFrame};
use super::hub_helpers::PaneState;
use crate::backend::PaneId;
use std::collections::VecDeque;
use std::mem::MaybeUninit;
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

/// Queue `chunks` for `pane`, reporting whether all of them fit. Stops at the
/// first frame the queue refuses: what follows would not fit either, and a replay
/// missing a piece in the middle is a screen the client cannot repair.
///
/// Shares each payload rather than copying it: the record was captured as these
/// frames' bytes already (see [`PaneReplay::capture`]).
fn send_replay(queue: &mut ReplayQueue<'_>, pane: PaneId, chunks: &[Arc<[u8]>]) -> bool {
    chunks.iter().all(|data| {
        queue.send(TerminalFrame::Output {
            pane,
            data: Arc::clone(data),
        })
    })
}

/// Where the bytes of a replay record go as it is composed.
///
/// The hub fills [`Chunks`], so a pane's bytes are copied once — straight into
/// the frame that will carry them. The tests compose into a plain buffer, which
/// is what lets them compare a record against the pane's own bytes.
pub(super) trait Sink {
    fn write(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

/// A replay record under construction: [`REPLAY_CHUNK_BYTES`]-sized [`Arc`]s,
/// filled in place.
///
/// The chunks are allocated up front, at exactly the lengths
/// `data.chunks(REPLAY_CHUNK_BYTES)` would cut them to, so the frames a client
/// receives are framed the same way as before — but each one already *is* the
/// allocation that gets queued, which is why capturing a replay costs one copy of
/// the pane's bytes instead of two.
struct Chunks {
    /// One per frame, filled from the front.
    chunks: Vec<Arc<[MaybeUninit<u8>]>>,
    /// The chunk receiving bytes, and how many it holds so far.
    open: usize,
    filled: usize,
    /// Bytes handed to `write`, to check every allocated byte was written.
    written: usize,
}

impl Chunks {
    fn new(total: usize) -> Self {
        let mut chunks = Vec::with_capacity(total.div_ceil(REPLAY_CHUNK_BYTES));
        let mut remaining = total;
        while remaining > 0 {
            let len = remaining.min(REPLAY_CHUNK_BYTES);
            chunks.push(Arc::new_uninit_slice(len));
            remaining -= len;
        }
        Self {
            chunks,
            open: 0,
            filled: 0,
            written: 0,
        }
    }

    fn finish(self) -> Vec<Arc<[u8]>> {
        debug_assert_eq!(
            self.written,
            self.chunks.iter().map(|chunk| chunk.len()).sum::<usize>(),
            "a replay record must be composed with the length it was sized for"
        );
        self.chunks
            .into_iter()
            .map(|chunk| {
                // SAFETY: `new` sized these for `written` bytes and `write` filled
                // them from the front in order, which the check above confirms;
                // each chunk is its allocation's only owner until here, so nothing
                // else could have observed the uninitialized bytes.
                unsafe { chunk.assume_init() }
            })
            .collect()
    }
}

impl Sink for Chunks {
    fn write(&mut self, mut bytes: &[u8]) {
        while !bytes.is_empty() {
            let capacity = self.chunks[self.open].len();
            let take = (capacity - self.filled).min(bytes.len());
            let chunk = Arc::get_mut(&mut self.chunks[self.open])
                .expect("a chunk being filled has one owner");
            // SAFETY: `take` bytes are copied into uninitialized space inside the
            // chunk's allocation at `filled`, which is below its length; the two
            // regions are exactly `take` bytes long and do not overlap.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    bytes.as_ptr(),
                    chunk.as_mut_ptr().add(self.filled).cast::<u8>(),
                    take,
                );
            }
            self.filled += take;
            self.written += take;
            if self.filled == capacity {
                self.open += 1;
                self.filled = 0;
            }
            bytes = &bytes[take..];
        }
    }
}

/// The bytes that rebuild a pane's normal screen: the ring up to the mark
/// (history — its screen state is superseded by the snapshot, but its scrolled-off
/// lines are still worth scrolling back to), the snapshot itself (an absolute
/// repaint, so whatever viewport the truncated history left behind is erased and
/// the screen arrives whole), then everything recorded since the snapshot was
/// taken. A pane with no snapshot yet has `covered == 0` and an empty
/// `normal_screen`, which makes this the plain ring.
fn push_normal_record(sink: &mut impl Sink, pane: &PaneState) {
    push_ring(sink, &pane.scrollback, 0..pane.covered);
    sink.write(&pane.normal_screen);
    push_ring(sink, &pane.scrollback, pane.covered..pane.scrollback.len());
}

/// Hand `range` of a deque's bytes to `sink`, in one write per contiguous slice.
pub(super) fn push_ring(sink: &mut impl Sink, ring: &VecDeque<u8>, range: Range<usize>) {
    let (front, back) = ring.as_slices();
    let start = range.start.min(ring.len());
    let end = range.end.min(ring.len()).max(start);
    if start < front.len() {
        sink.write(&front[start..end.min(front.len())]);
    }
    if end > front.len() {
        let skipped = start.saturating_sub(front.len());
        sink.write(&back[skipped..end - front.len()]);
    }
}

/// One pane's replay, captured under the hub's state lock: the bytes of its
/// records are copied once, directly into the frames that will carry them, so
/// queueing the replay after the lock is released costs nothing but clones.
pub(super) struct PaneReplay {
    pub(super) pane: PaneId,
    rows: u16,
    cols: u16,
    title: Option<String>,
    /// The normal record to put on the normal buffer first, for a pane whose
    /// program has left it (see [`replay_pane`]). Empty for a pane with nothing
    /// frozen underneath.
    normal_under: Vec<Arc<[u8]>>,
    prelude: Vec<Arc<[u8]>>,
    record: Vec<Arc<[u8]>>,
}

impl PaneReplay {
    pub(super) fn capture(pane: &PaneState) -> Self {
        let normal_under = if pane.modes.alt_screen
            && !(pane.scrollback.is_empty() && pane.normal_screen.is_empty())
        {
            let mut chunks = Chunks::new(
                LEAVE_ALT_SCREEN.len() + pane.scrollback.len() + pane.normal_screen.len(),
            );
            chunks.write(LEAVE_ALT_SCREEN);
            push_normal_record(&mut chunks, pane);
            chunks.finish()
        } else {
            Vec::new()
        };
        let record = if pane.modes.alt_screen {
            let mut chunks = Chunks::new(pane.screen.len() + pane.since.len());
            chunks.write(&pane.screen);
            push_ring(&mut chunks, &pane.since, 0..pane.since.len());
            chunks.finish()
        } else {
            let mut chunks = Chunks::new(pane.scrollback.len() + pane.normal_screen.len());
            push_normal_record(&mut chunks, pane);
            chunks.finish()
        };
        let prelude = pane.modes.prelude();
        let mut prelude_chunks = Chunks::new(prelude.len());
        prelude_chunks.write(&prelude);
        Self {
            pane: pane.id,
            rows: pane.rows,
            cols: pane.cols,
            title: pane.title.clone(),
            normal_under,
            prelude: prelude_chunks.finish(),
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
    if !replay.normal_under.is_empty() {
        whole &= send_replay(queue, replay.pane, &replay.normal_under);
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
