//! Codex adapter.
//!
//! Codex has no hook, no statusline and no `status` subcommand, and it *exits*
//! when the usage limit is hit — with exit code 1, indistinguishable from any
//! other failure — so neither the exit code nor a still-running process can be
//! used as a signal. What codex does have is a per-session rollout file, and that
//! is the primary source here: [`Provider::poll`] tails the pane's rollout and
//! acts on the `turn_complete` record whose `error.codex_error_info` is
//! `usage_limit_exceeded`, taking the deadline from the most recent `token_count`
//! record. `EventMsg::Error` is not persisted to the rollout, so it is not looked
//! for. Terminal text is a documented fallback only, and a reset time is never
//! parsed out of it.
//!
//! Recovery is always a relaunch (`codex resume <SESSION_ID>`), never typed
//! input. `codex resume --last` is deliberately never used: nightcrow allows
//! several codex panes on one repository, so "the last session" could belong to
//! another pane. Without an unambiguous session id this adapter holds.
//!
//! Layout: `codex_pane.rs` holds the per-pane watching state,
//! `codex_sessions.rs` finds the pane's rollout file and `codex_rollout.rs` holds
//! the pure record grammar. This file holds only the `Provider` contract.

use super::{LimitEvent, PaneContext, Provider, ResumePlan};
use crate::protocol::PaneToken;
use pane::PaneState;
use rollout::valid_session_id;
use std::collections::HashMap;
use std::path::PathBuf;

#[path = "codex_pane.rs"]
mod pane;
#[path = "codex_rollout.rs"]
mod rollout;
#[path = "codex_sessions.rs"]
mod sessions;

/// Env var codex reads for its state directory.
const CODEX_HOME_ENV: &str = "CODEX_HOME";
/// Env var giving the home directory the default state directory hangs off.
const HOME_ENV: &str = "HOME";
/// `CODEX_HOME` defaults to this directory under `$HOME`.
const DEFAULT_HOME_DIR: &str = ".codex";
/// Rollout files live under `<CODEX_HOME>/sessions/<YYYY>/<MM>/<DD>/`.
const SESSIONS_DIR: &str = "sessions";

/// First arg of the relaunch. The host supplies the program, so this can never
/// name a different binary.
const RESUME_SUBCOMMAND: &str = "resume";

const HOLD_ALIVE: &str = "codex is still running; nothing to resume";
const HOLD_NO_ID: &str =
    "no unambiguous codex session id; --last could resume another pane's session";

#[derive(Debug)]
pub struct Codex {
    home: PathBuf,
    panes: HashMap<PaneToken, PaneState>,
}

impl Default for Codex {
    fn default() -> Self {
        let home = std::env::var_os(CODEX_HOME_ENV)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os(HOME_ENV)
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join(DEFAULT_HOME_DIR))
            })
            .unwrap_or_else(|| PathBuf::from(DEFAULT_HOME_DIR));
        Self::with_home(home)
    }
}

impl Codex {
    /// Testing seam: point the adapter at a specific `CODEX_HOME`.
    pub fn with_home(home: PathBuf) -> Self {
        Self {
            home,
            panes: HashMap::new(),
        }
    }

    /// This pane's state, reset when the pane has been respawned: a new
    /// generation is a new codex process writing a new rollout, so nothing about
    /// the old one may carry over.
    fn state_for(&mut self, ctx: &PaneContext, now_epoch: i64) -> &mut PaneState {
        let state = self
            .panes
            .entry(ctx.token.clone())
            .or_insert_with(|| PaneState::new(ctx.generation, now_epoch));
        if state.generation() != ctx.generation {
            *state = PaneState::new(ctx.generation, now_epoch);
        }
        state
    }

    fn session_id_for(&self, ctx: &PaneContext) -> Option<String> {
        self.panes
            .get(&ctx.token)
            .filter(|state| state.generation() == ctx.generation)
            .and_then(PaneState::session_id)
    }
}

impl Provider for Codex {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn poll(&mut self, ctx: &PaneContext, now_epoch: i64) -> Option<LimitEvent> {
        let sessions = self.home.join(SESSIONS_DIR);
        self.state_for(ctx, now_epoch).tail(&sessions, now_epoch)
    }

    fn on_output(&mut self, ctx: &PaneContext, text: &str, now_epoch: i64) -> Option<LimitEvent> {
        self.state_for(ctx, now_epoch).on_output(text)
    }

    fn on_exit(&mut self, ctx: &PaneContext) {
        let Some(state) = self.panes.get_mut(&ctx.token) else {
            return;
        };
        if state.generation() != ctx.generation {
            return;
        }
        state.rearm_output();
    }

    fn resume(&self, ctx: &PaneContext, limit: &LimitEvent, alive: bool) -> Option<ResumePlan> {
        if alive {
            // Codex exits on a usage limit, so a live process is either working
            // or waiting on the user; there is nothing to resume and typed input
            // would land in whatever it is doing.
            return Some(ResumePlan::Hold(HOLD_ALIVE));
        }
        let id = limit
            .session_id
            .clone()
            .or_else(|| self.session_id_for(ctx))
            .filter(|id| valid_session_id(id));
        match id {
            Some(id) => Some(ResumePlan::Relaunch(vec![
                RESUME_SUBCOMMAND.to_string(),
                id,
            ])),
            None => Some(ResumePlan::Hold(HOLD_NO_ID)),
        }
    }
}

#[cfg(test)]
#[path = "codex_output_tests.rs"]
mod output_tests;
#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
