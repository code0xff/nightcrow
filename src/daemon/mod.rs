//! The session daemon's own transport: a Unix socket carrying framed messages
//! to attaching clients.
//!
//! Separate from `web/` because the audiences differ in kind, not just in
//! encoding. The browser reaches a TCP port that anything routable can also
//! reach, so it authenticates; an attaching client reaches a socket file only
//! the user can open, so it does not. Keeping the two transports apart is what
//! keeps that difference from becoming a mistake in a shared code path.

use std::time::Duration;

/// Maximum time the daemon spends waiting for a pre-attach handshake frame.
/// Stateful attach clears this timeout as soon as `Hello` is accepted.
pub(crate) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) mod client;
pub(crate) mod clients;
pub(crate) mod detach;
pub(crate) mod frame;
pub(crate) mod lock;
pub(crate) mod one_shot;
pub(crate) mod protocol;
pub(crate) mod requests;
pub(crate) mod serve;
pub(crate) mod socket;
mod status;
pub(crate) mod terminal_link;
pub(crate) mod terminals;
pub(crate) mod transport;
pub(crate) mod watch;
pub(crate) mod wire;
