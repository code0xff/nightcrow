//! The recovery state machine, one module per group of behaviours, sharing the
//! fixtures below. A fake adapter stands in for a provider: the machine's
//! contract with an adapter is `resume`, so that is all the fake implements.

use super::*;
use crate::protocol::PROTOCOL_VERSION;
use crate::provider::{LimitEvent, PaneContext, Provider, ResumePlan};
use crate::wait::{BACKOFF_BASE_SECS, RESET_GRACE_SECS};
use std::time::Duration;

mod cancel;
mod resume;
mod transitions;

pub(super) const TOKEN: &str = "0123456789abcdef0123456789abcdef";
pub(super) const OTHER_TOKEN: &str = "ffffffffffffffffffffffffffffffff";
pub(super) const SESSION: &str = "11111111-2222-3333-4444-555555555555";
pub(super) const OTHER_SESSION: &str = "99999999-8888-7777-6666-555555555555";
/// A readable fixed "now": 2026-01-01T00:00:00Z.
pub(super) const T0: i64 = 1_767_225_600;
/// A reset an hour out, so a wait is unambiguously in the future.
pub(super) const RESET: i64 = T0 + 3600;

/// An adapter that answers whatever the test told it to, so a test spoils
/// exactly one thing.
#[derive(Debug)]
pub(super) struct FakeProvider {
    pub alive_plan: Option<ResumePlan>,
    pub exited_plan: Option<ResumePlan>,
}

impl Default for FakeProvider {
    fn default() -> Self {
        Self {
            alive_plan: Some(ResumePlan::Hold("still running")),
            exited_plan: Some(ResumePlan::Relaunch(vec![
                "--resume".to_string(),
                SESSION.to_string(),
            ])),
        }
    }
}

impl FakeProvider {
    /// An adapter that will only ever relaunch, which is the codex/opencode
    /// shape: those providers exit when they hit a limit.
    pub fn relaunch_only() -> Self {
        Self {
            alive_plan: Some(ResumePlan::Hold("still running")),
            ..Self::default()
        }
    }
}

impl Provider for FakeProvider {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn resume(&self, _ctx: &PaneContext, _limit: &LimitEvent, alive: bool) -> Option<ResumePlan> {
        if alive {
            self.alive_plan.clone()
        } else {
            self.exited_plan.clone()
        }
    }
}

pub(super) fn ctx() -> PaneContext {
    PaneContext {
        token: TOKEN.to_string(),
        generation: 1,
        cwd: "/w/repo".to_string(),
        command: Some("codex".to_string()),
    }
}

pub(super) fn recovery() -> PaneRecovery {
    PaneRecovery::new(TOKEN.to_string(), 1)
}

pub(super) fn usage(session: Option<&str>, resets_at: Option<i64>) -> LimitEvent {
    LimitEvent::usage(session.map(str::to_string), resets_at, "test limit")
}

pub(super) fn opened(generation: PaneGeneration) -> PluginEvent {
    PluginEvent::PaneOpened {
        v: PROTOCOL_VERSION,
        token: TOKEN.to_string(),
        generation,
        title: None,
        command: Some("codex".to_string()),
        cwd: "/w/repo".to_string(),
    }
}

pub(super) fn went_idle(generation: PaneGeneration) -> PluginEvent {
    PluginEvent::PaneIdle {
        v: PROTOCOL_VERSION,
        token: TOKEN.to_string(),
        generation,
        idle_ms: 30_000,
    }
}

pub(super) fn exited(generation: PaneGeneration) -> PluginEvent {
    PluginEvent::PaneExited {
        v: PROTOCOL_VERSION,
        token: TOKEN.to_string(),
        generation,
    }
}

pub(super) fn closed(generation: PaneGeneration) -> PluginEvent {
    PluginEvent::PaneClosed {
        v: PROTOCOL_VERSION,
        token: TOKEN.to_string(),
        generation,
    }
}

pub(super) fn user_input(generation: PaneGeneration) -> PluginEvent {
    PluginEvent::UserInput {
        v: PROTOCOL_VERSION,
        token: TOKEN.to_string(),
        generation,
    }
}

/// Advance the machine as the run loop would: once, at this moment.
pub(super) fn tick_at(
    rec: &mut PaneRecovery,
    provider: &dyn Provider,
    epoch: i64,
    mono: Instant,
) -> Vec<PluginCommand> {
    rec.tick(provider, &ctx(), epoch, mono)
}

/// The one command a test cares about, or `None` if the machine only reported
/// status.
pub(super) fn action(commands: &[PluginCommand]) -> Option<&PluginCommand> {
    commands
        .iter()
        .find(|c| !matches!(c, PluginCommand::Status { .. }))
}

pub(super) fn states(commands: &[PluginCommand]) -> Vec<String> {
    commands
        .iter()
        .filter_map(|c| match c {
            PluginCommand::Status { state, .. } => Some(state.clone()),
            _ => None,
        })
        .collect()
}
