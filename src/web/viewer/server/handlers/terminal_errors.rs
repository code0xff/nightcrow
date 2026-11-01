//! Telling a socket that stalled apart from one that is gone.
//!
//! Every write in the connection loop goes through this judgement: waiting out
//! a stall is what keeps a page that slept for a while from losing every pane,
//! while continuing past a real failure would keep a dead connection's hub
//! client and connection slot alive.

/// Whether a failed socket operation leaves the connection usable.
///
/// A timeout is not a departure: it surfaces as `WouldBlock` on macOS and
/// `TimedOut` on Linux, and ending the connection there cost a page that
/// stopped reading for fifteen seconds (a phone asleep, a tunnel
/// renegotiating) every pane replayed from scratch. tungstenite draws the
/// same line: an `Io` error is fatal "except for WouldBlock", and the frame
/// that could not go out stays in its write buffer for the next flush.
///
/// A client that has genuinely stopped keeping up is still cut off — by the
/// hub, once its queue fills (`broadcast_locked`). That is where the cap
/// belongs; a single slow write is not evidence of one.
pub(super) fn stalled_not_gone(err: &tungstenite::Error) -> bool {
    matches!(
        err,
        tungstenite::Error::Io(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            )
    )
}

/// Say why a terminal socket ended.
///
/// At INFO, and unconditional: the page answers a closed socket by replaying
/// every pane from scratch, which a person sees, and nothing else in the log
/// says it happened.
pub(super) fn note_end(err: &tungstenite::Error, during: &'static str) {
    tracing::info!(%err, during, "viewer: terminal socket ended");
}

#[cfg(test)]
#[path = "terminal_errors_tests.rs"]
mod tests;
