//! The three answers a quiet page can produce, and when each is due.

use super::{Liveness, Step, Timing};
use std::time::{Duration, Instant};

fn timing() -> Timing {
    Timing {
        ping_after: Duration::from_secs(15),
        silence_limit: Duration::from_secs(30),
    }
}

#[test]
fn a_fresh_connection_has_nothing_to_answer_for() {
    let start = Instant::now();
    let liveness = Liveness::new(timing(), start);
    assert_eq!(liveness.step(start), Step::Wait);
    assert_eq!(liveness.step(start + Duration::from_secs(14)), Step::Wait);
}

/// The page answers pings on its own; asking is how a dead path gets a chance
/// to say so before the connection is ended.
#[test]
fn fifteen_seconds_without_a_frame_from_the_page_asks_it_to_answer() {
    let start = Instant::now();
    let liveness = Liveness::new(timing(), start);
    assert_eq!(liveness.step(start + Duration::from_secs(15)), Step::Ping);
}

/// The ping itself is traffic. Without recording it the loop would queue one
/// more on every pass.
#[test]
fn a_ping_that_went_out_restarts_the_wait() {
    let start = Instant::now();
    let mut liveness = Liveness::new(timing(), start);
    let sent_at = start + Duration::from_secs(15);
    assert_eq!(liveness.step(sent_at), Step::Ping);
    liveness.pinged(sent_at);
    assert_eq!(liveness.step(sent_at), Step::Wait);
    assert_eq!(liveness.step(sent_at + Duration::from_secs(14)), Step::Wait);
    assert_eq!(
        liveness.step(sent_at + Duration::from_secs(15)),
        Step::Close,
        "an unanswered ping leaves the socket silent for the whole limit"
    );
}

/// A pong, a keystroke, anything at all: the path is carrying frames.
#[test]
fn any_frame_from_the_page_keeps_the_socket_alive() {
    let start = Instant::now();
    let mut liveness = Liveness::new(timing(), start);
    liveness.heard(start + Duration::from_secs(29));
    assert_ne!(
        liveness.step(start + Duration::from_secs(30)),
        Step::Close,
        "an answer 29 seconds in must not be treated as silence"
    );
    assert_eq!(
        liveness.next_deadline(),
        start + Duration::from_secs(44),
        "the ping is due an interval after the page was last heard, not after the last write"
    );
    liveness.pinged(start + Duration::from_secs(44));
    assert_eq!(
        liveness.step(start + Duration::from_secs(59)),
        Step::Close,
        "nothing has arrived for the whole limit"
    );
}

/// Closing outranks pinging: once the limit is reached, another question would
/// only be sent to a socket nobody is reading.
#[test]
fn the_silence_limit_is_answered_with_a_close_not_another_ping() {
    let start = Instant::now();
    let liveness = Liveness::new(timing(), start);
    assert_eq!(liveness.step(start + Duration::from_secs(30)), Step::Close);
}

#[test]
fn the_deadline_is_whichever_decision_comes_first() {
    let start = Instant::now();

    let mut quiet = Liveness::new(timing(), start);
    assert_eq!(quiet.next_deadline(), start + Duration::from_secs(15));

    // A frame from the page moves both clocks, so the ping is due a full
    // interval after the page was last heard.
    quiet.heard(start + Duration::from_secs(20));
    assert_eq!(quiet.next_deadline(), start + Duration::from_secs(35));

    // A ping later than the last inbound frame moves only the ping clock.
    quiet.pinged(start + Duration::from_secs(36));
    assert_eq!(
        quiet.next_deadline(),
        start + Duration::from_secs(50),
        "the silence limit can be the nearer decision"
    );
}
