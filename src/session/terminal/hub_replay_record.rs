//! Composing one pane's replay record: where its bytes come from, and putting
//! them into the frames that will carry them. The order itself is described by
//! [`PaneReplay`](super::hub_replay::PaneReplay).

use super::hub_helpers::PaneState;
use super::hub_replay::REPLAY_CHUNK_BYTES;
use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::ops::Range;
use std::sync::Arc;

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
pub(super) struct Chunks {
    /// One per frame, filled from the front.
    chunks: Vec<Arc<[MaybeUninit<u8>]>>,
    /// The chunk receiving bytes, and how many it holds so far.
    open: usize,
    filled: usize,
    /// Bytes handed to `write`, to check every allocated byte was written.
    written: usize,
}

impl Chunks {
    pub(super) fn new(total: usize) -> Self {
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

    pub(super) fn finish(self) -> Vec<Arc<[u8]>> {
        // Not a debug assertion: the `unsafe` below is only sound when every
        // allocated byte was written, so a release build has to check it too. The
        // sum is over a handful of lengths, which is nothing next to the copy that
        // filled them.
        assert_eq!(
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
pub(super) fn push_normal_record(sink: &mut impl Sink, pane: &PaneState) {
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
