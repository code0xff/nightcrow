//! Asking the host for a pane it never named to us.
//!
//! The dominant way a coding CLI gets started is by hand: the user opens a
//! plain shell and types `claude` into it. That pane's `[[startup_command]]`
//! names no plugin, so the host never mentions it — but the CLI's hook still
//! reaches us over the socket, carrying the token the host put in that pane's
//! environment. Presenting the token back is the whole request; the host
//! decides whether to honour it, and never tells us when it does not.
//!
//! Everything here exists because of that silence: a refusal is
//! indistinguishable from a token belonging to another nightcrow session, so
//! an unanswered request must not be repeated in a tight loop and must not
//! leave state behind that grows with every stranger that knocks.

use crate::ipc::IpcMessage;
use crate::protocol::{PaneToken, PluginCommand, watch_pane};
use crate::provider::OutOfBand;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Most tokens we will have an outstanding request for at once.
///
/// These are unsolicited: anything that can reach the socket can name a token we
/// have never seen. A session has a handful of panes and the host answers within
/// one of its ticks, so a backlog this deep already means the requests are not
/// being honoured — and dropping the newest is what keeps a stream of strangers
/// from growing this map.
const MAX_PENDING: usize = 8;

/// How long one token's unanswered request suppresses another for that token.
///
/// The host answers in milliseconds or never, so this is not a retry interval —
/// it is the rate at which a token that is not ours may cost us a command.
/// Claude Code's statusline runs on every render, so without it a foreign pane
/// would have us writing a refused request several times a second, each counted
/// against the host's per-tick command budget. Half a minute is far longer than
/// any honoured request takes and short enough that a pane which only just
/// became ours is not shut out for long.
const REQUEST_COOLDOWN: Duration = Duration::from_secs(30);

/// One outstanding request, and the signal that justified making it.
struct Pending {
    /// Kept so the adapter still gets it. The signal arrives *before* the pane
    /// does — it is the reason the pane arrives at all — and the host replays
    /// no history to a pane it has just handed over, so dropping it would lose
    /// the very limit being recovered from.
    signal: OutOfBand,
    asked_at: Instant,
}

/// The tokens we have asked about and not yet been given.
#[derive(Default)]
pub(crate) struct Adoptions(HashMap<PaneToken, Pending>);

impl Adoptions {
    /// Turn a signal for an untracked pane into a request, or into nothing.
    ///
    /// Consumes the message either way: when the answer is nothing, the token and
    /// its payload are dropped here rather than recorded, so a token that will
    /// never be honoured costs one hash probe and no growth.
    pub(crate) fn request(&mut self, msg: IpcMessage, now: Instant) -> Option<PluginCommand> {
        let (token, signal) = msg.into_signal();
        // Already asked, and [`Self::prune`] has not yet given up on it.
        if self.0.contains_key(&token) {
            return None;
        }
        if self.0.len() >= MAX_PENDING {
            return None;
        }
        self.0.insert(
            token.clone(),
            Pending {
                signal,
                asked_at: now,
            },
        );
        Some(watch_pane(token))
    }

    /// Take back the signal that won `token` its request, now that the host has
    /// described the pane. Answers `None` for a pane we never asked about, which
    /// is every configured pane.
    pub(crate) fn claim(&mut self, token: &str) -> Option<OutOfBand> {
        self.0.remove(token).map(|pending| pending.signal)
    }

    /// Give up on requests the host has not answered within
    /// [`REQUEST_COOLDOWN`], which is also what lets that token be asked about
    /// again.
    ///
    /// Without it a handful of foreign tokens would hold every slot for the
    /// process's whole life. Giving up is safe: a pane that really is ours
    /// signals again, and the held signal is stale by then anyway.
    pub(crate) fn prune(&mut self, now: Instant) {
        self.0
            .retain(|_, p| now.saturating_duration_since(p.asked_at) < REQUEST_COOLDOWN);
    }
}

#[cfg(test)]
#[path = "runloop_adopt_tests.rs"]
mod tests;
