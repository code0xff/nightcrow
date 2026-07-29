//! The session daemon's own transport: a Unix socket carrying framed messages
//! to attaching clients.
//!
//! Separate from `web/` because the audiences differ in kind, not just in
//! encoding. The browser reaches a TCP port that anything routable can also
//! reach, so it authenticates; an attaching client reaches a socket file only
//! the user can open, so it does not. Keeping the two transports apart is what
//! keeps that difference from becoming a mistake in a shared code path.

// The transport lands before the accept loop that drives it, so nothing calls
// into it yet. Removed once the daemon listens (step C of
// `docs/session-daemon-plan.md`).
#![allow(dead_code)]

pub(crate) mod frame;
pub(crate) mod socket;
