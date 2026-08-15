//! The byte ring's eviction contract: bounded from the front, never past the
//! `covered` mark. What the mark means to a replay is pinned in
//! [`normal_records`](super::normal_records); this is the pure mechanics.

use crate::session::limits;
use crate::session::terminal::hub_helpers::push_scrollback;
use std::collections::VecDeque;

#[test]
fn scrollback_is_bounded_and_keeps_the_most_recent_bytes() {
    let cap = limits::MAX_TERMINAL_SCROLLBACK_BYTES;
    let mut buf = VecDeque::new();
    let mut covered = 0;
    for _ in 0..(cap / 1000 + 5) {
        if push_scrollback(&mut buf, &mut covered, &vec![b'x'; 1000]) > cap {
            // What the worker does on the spot: a fresh snapshot moves the mark.
            covered = buf.len();
        }
    }
    assert!(
        buf.len() <= cap + 1000,
        "scrollback must be capped, allowing one chunk of overrun before a snapshot"
    );

    // The tail past the mark is what a replay applies on top of the snapshot, so
    // the newest bytes must survive eviction.
    push_scrollback(&mut buf, &mut covered, b"TAIL");
    let contents: Vec<u8> = buf.iter().copied().collect();
    assert!(contents.ends_with(b"TAIL"), "newest bytes must be retained");
}

#[test]
fn scrollback_never_evicts_past_the_covered_mark() {
    let cap = limits::MAX_TERMINAL_SCROLLBACK_BYTES;
    let mut buf = VecDeque::new();
    let mut covered = 0;
    push_scrollback(&mut buf, &mut covered, b"HISTORY");
    // A snapshot covering everything so far.
    covered = buf.len();

    // A tail that outgrows the cap on its own: the covered history is spent, and
    // then the ring runs over rather than dropping a byte nothing else carries.
    let owed = push_scrollback(&mut buf, &mut covered, &vec![b'y'; cap + 1]);

    assert!(
        owed > cap,
        "a tail past the cap must ask for a fresh snapshot"
    );
    assert_eq!(covered, 0, "the covered history is what eviction spends");
    assert!(
        buf.iter().all(|&b| b == b'y'),
        "the uncovered tail must survive intact"
    );
}
