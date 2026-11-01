//! When a batch of queued frames has to reach the socket.

use super::{Batch, FLUSH_AFTER, FLUSH_BYTES, FLUSH_FRAMES};
use std::time::Duration;

#[test]
fn a_new_batch_holds_frames_until_a_flush() {
    let mut batch = Batch::default();
    assert!(!batch.queued(), "nothing queued yet");

    batch.add(1);

    assert!(batch.queued(), "the frame is waiting");
    assert!(!batch.due(), "one small frame is not a reason to flush");
}

#[test]
fn a_full_batch_is_due_without_the_burst_ending() {
    let mut batch = Batch::default();
    for _ in 0..FLUSH_FRAMES {
        batch.add(1);
    }

    assert!(
        batch.due(),
        "a burst must not hold more than the frame bound waiting for a pause"
    );
}

#[test]
fn a_large_batch_is_due_on_bytes() {
    let mut batch = Batch::default();
    batch.add(FLUSH_BYTES);

    assert!(batch.due(), "the byte bound is reached on its own");
}

/// A frame that has sat in the batch for a while must not wait for the burst to
/// end: a program that never pauses never ends one.
#[test]
fn a_batch_that_has_waited_is_due() {
    let mut batch = Batch::default();
    batch.add(1);
    assert!(!batch.due());

    std::thread::sleep(FLUSH_AFTER + Duration::from_millis(20));
    batch.add(1);

    assert!(batch.due(), "the age bound is reached on the next frame");
}

#[test]
fn a_flushed_batch_starts_empty() {
    let mut batch = Batch::default();
    batch.add(FLUSH_BYTES);
    assert!(batch.due());

    batch.clear();

    assert!(!batch.queued());
    assert!(!batch.due());
}
