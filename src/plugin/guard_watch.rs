//! Rule 12: when a plugin may be given a pane nobody handed it.
//!
//! Every other rule starts from a pane the operator already assigned; this is
//! the single place an assignment can be *created* at runtime, so it is kept
//! apart and reads as one list of conditions.
//!
//! What makes it safe is where the token came from: a pane token is random,
//! minted per slot, and put only into that pane's child environment, so a
//! process able to quote one is running inside that pane. The pane's own
//! occupant asking for a watcher is allowed here; a plugin enumerating the
//! session is not — nothing in this file looks at a list of panes. Still not
//! authority by itself: the operator's config switch must be on, and a pane
//! already spoken for is not taken away.
use super::guard::{Approved, PaneFacts};
use super::guard_refusal::Refused;
use crate::backend::PaneToken;

/// Decide one [`PluginCommand::WatchPane`](super::protocol::PluginCommand).
///
/// Takes no clock and charges no budget: being given a pane changes who is
/// told about it, not the pane itself — every act that follows is charged
/// when it is asked for. Charging here would spend the very allowance the
/// recovery this unlocks is about to need.
pub(super) fn judge_watch(
    token: &PaneToken,
    facts: Option<&PaneFacts>,
) -> Result<Approved, Refused> {
    let Some(facts) = facts else {
        // Overwhelmingly the ordinary case: a token from another nightcrow
        // session's panes, reaching this plugin because both are the same user.
        return Err(Refused::UnknownPane {
            token: token.clone(),
        });
    };
    if !facts.may_watch_on_signal {
        return Err(Refused::WatchNotAllowed {
            pane: facts.pane,
            token: token.clone(),
        });
    }
    if facts.watched_by_another {
        // One pane, one watcher. Two plugins driving the same keyboard would
        // interleave their recoveries, and the second would be acting on a pane
        // whose state the first is changing underneath it.
        return Err(Refused::PaneWatchedByAnother { pane: facts.pane });
    }
    if !facts.alive {
        // Nothing left to watch: the slot may still exist, but the process that
        // proved what the pane was running has gone, and the only recovery a
        // pane taken on this way can get is typed into a live process.
        return Err(Refused::PaneNotRunning { pane: facts.pane });
    }
    // Deliberately allowed when this plugin already has the pane. That happens
    // when an opted-in pane was handed over but its occupant could not be
    // recognised from the command line, and asking again is how the plugin gets
    // the `PaneOpened` it needs to try once more.
    Ok(Approved::WatchPane { pane: facts.pane })
}
