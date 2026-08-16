//! Guard rules, one module per group of rules, sharing the fixtures below.

use super::*;
use crate::plugin::guard_text::MAX_LOG_MESSAGE_BYTES;
use crate::plugin::protocol::PROTOCOL_VERSION;

mod budget;
mod identity;
mod input;
mod relaunch;
mod watch;

pub(super) const PANE: PaneId = 4;
pub(super) const GENERATION: PaneGeneration = 9;
pub(super) const MIN_IDLE: Duration = Duration::from_secs(10);
pub(super) const LAUNCH: &str = "provider-cli";

pub(super) fn token() -> PaneToken {
    PaneToken::new().expect("OS RNG")
}

pub(super) fn guard() -> Guard {
    Guard::new(MIN_IDLE, RateLimits::default())
}

/// A pane that passes every precondition, so each test spoils exactly one.
pub(super) fn facts() -> PaneFacts {
    PaneFacts {
        pane: PANE,
        generation: GENERATION,
        opted_in: true,
        watched_by_another: false,
        may_watch_on_signal: false,
        alive: true,
        idle: MIN_IDLE,
        launch_command: Some(LAUNCH.to_string()),
    }
}

/// A live pane no plugin has yet, in a session where the asking plugin is
/// allowed to ask — the one shape [`watch`] is meant to be approved for.
pub(super) fn adoptable_facts() -> PaneFacts {
    PaneFacts {
        opted_in: false,
        may_watch_on_signal: true,
        // A pane somebody started a CLI in by hand: the host launched no command
        // of its own in it.
        launch_command: None,
        ..facts()
    }
}

/// The same pane after its process ended, which is what a relaunch needs.
pub(super) fn exited_facts() -> PaneFacts {
    PaneFacts {
        alive: false,
        ..facts()
    }
}

pub(super) fn send(token: &PaneToken, data: &str) -> PluginCommand {
    PluginCommand::SendInput {
        v: PROTOCOL_VERSION,
        token: token.clone(),
        generation: GENERATION,
        data: data.to_string(),
    }
}

pub(super) fn relaunch(token: &PaneToken, args: &[&str]) -> PluginCommand {
    PluginCommand::Relaunch {
        v: PROTOCOL_VERSION,
        token: token.clone(),
        generation: GENERATION,
        resume_args: args.iter().map(|a| a.to_string()).collect(),
    }
}

pub(super) fn status(token: &PaneToken) -> PluginCommand {
    PluginCommand::Status {
        v: PROTOCOL_VERSION,
        token: token.clone(),
        generation: GENERATION,
        state: "waiting".to_string(),
        detail: None,
        deadline_epoch: Some(42),
        attempt: 2,
    }
}

pub(super) fn attention(token: &PaneToken) -> PluginCommand {
    PluginCommand::Attention {
        v: PROTOCOL_VERSION,
        token: token.clone(),
        generation: GENERATION,
    }
}

pub(super) fn watch(token: &PaneToken) -> PluginCommand {
    PluginCommand::WatchPane {
        v: PROTOCOL_VERSION,
        token: token.clone(),
    }
}

pub(super) fn flags(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| n.to_string()).collect()
}
