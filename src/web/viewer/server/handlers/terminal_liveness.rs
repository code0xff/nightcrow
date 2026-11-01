//! How long a terminal socket may stay quiet, and what to do about it.
//!
//! A page whose network path vanished leaves a socket that never reports an
//! error: half a connection stays ESTABLISHED on this side until the kernel
//! gives up on a write. Output alone cannot settle it either, because a busy
//! pane keeps succeeding writes into a peer that is no longer there. So the
//! server asks, and treats silence as an answer of its own.

use std::time::{Duration, Instant};

/// Silence on the way out before the server pings the page.
pub const PING_AFTER: Duration = Duration::from_secs(15);

/// Silence of every kind — output, input, or an answer to a ping — before the
/// socket is taken for dead. Two ping intervals: one missed wake-up is a
/// throttled tab or a sleeping phone, not a page that has gone.
pub const SILENCE_LIMIT: Duration = Duration::from_secs(30);

/// The two intervals a connection's liveness turns on.
///
/// Its own type rather than constants read directly, so a test can watch a
/// whole timeout pass in a fraction of a second instead of sleeping for thirty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub ping_after: Duration,
    pub silence_limit: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            ping_after: PING_AFTER,
            silence_limit: SILENCE_LIMIT,
        }
    }
}

/// What the connection should do about the silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing yet.
    Wait,
    /// Ask the page to prove it is still there.
    Ping,
    /// Nothing has arrived; stop treating the socket as alive.
    Close,
}

/// The traffic clock behind one terminal socket.
pub struct Liveness {
    timing: Timing,
    last_outbound: Instant,
    last_inbound: Instant,
}

impl Liveness {
    pub fn new(timing: Timing, now: Instant) -> Self {
        Self {
            timing,
            last_outbound: now,
            last_inbound: now,
        }
    }

    /// A frame went out, in full or into the socket's write buffer.
    pub fn sent(&mut self, now: Instant) {
        self.last_outbound = now;
    }

    /// A frame arrived: pane input, an answer to a ping, or a close.
    pub fn heard(&mut self, now: Instant) {
        self.last_inbound = now;
    }

    /// What the silence calls for now.
    pub fn step(&self, now: Instant) -> Step {
        if now.saturating_duration_since(self.last_inbound) >= self.timing.silence_limit {
            Step::Close
        } else if now.saturating_duration_since(self.last_outbound) >= self.timing.ping_after {
            Step::Ping
        } else {
            Step::Wait
        }
    }

    /// How long silence is allowed to last before the socket is closed.
    pub fn silence_limit(&self) -> Duration {
        self.timing.silence_limit
    }

    /// When [`step`](Self::step) would change without further traffic. A
    /// connection that waits until here is not polling: it is asleep until
    /// something arrives or the next decision is due.
    pub fn next_deadline(&self) -> Instant {
        (self.last_outbound + self.timing.ping_after)
            .min(self.last_inbound + self.timing.silence_limit)
    }
}

#[cfg(test)]
#[path = "terminal_liveness_tests.rs"]
mod tests;
