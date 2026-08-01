//! The trust boundary between an untrusted plugin and the panes.
//!
//! [`decode_command`](super::protocol::decode_command) checked shape and bounds;
//! it never checked authority. Everything a plugin asks for passes through
//! [`Guard::judge`], which returns either an [`Approved`] action naming a real
//! [`PaneId`] or a [`Refused`] saying why not. There is no other way through.
//!
//! That includes gaining a pane in the first place: `guard_watch` beside this
//! file holds the only rule that can widen what a plugin is allowed to see.

use super::guard_budget::{Budgets, RateAction, RateLimits};
use super::guard_refusal::Refused;
use super::guard_text::{is_forbidden_control, truncate_message};
use super::guard_watch::judge_watch;
use super::protocol::{LogLevel, MAX_INPUT_BYTES, PluginCommand};
use crate::backend::slot::resume_command_line;
use crate::backend::{PaneGeneration, PaneId, PaneToken};
use std::time::{Duration, Instant};

/// What the guard needs to know about the pane a command names.
///
/// Supplied by the caller rather than looked up here, so every rule is testable
/// without a PTY and the guard holds no reference to the backend.
#[derive(Debug, Clone)]
pub struct PaneFacts {
    pub pane: PaneId,
    pub generation: PaneGeneration,
    /// This plugin already has the pane, by opt-in or by an earlier
    /// [`PluginCommand::WatchPane`].
    pub opted_in: bool,
    /// Some *other* plugin has the pane. Distinct from `!opted_in`, which is
    /// equally true of a pane no plugin has at all — and those two answer
    /// [`PluginCommand::WatchPane`] differently.
    pub watched_by_another: bool,
    /// The requesting plugin's `watch_on_signal`: whether the operator allowed
    /// it to be given a pane it was never named by.
    pub may_watch_on_signal: bool,
    /// The pane's process is still running.
    pub alive: bool,
    /// Since the pane last produced output.
    pub idle: Duration,
    /// The pane's configured startup command, or `None` for a bare shell.
    ///
    /// Needed because a relaunch is validated by calling the real
    /// [`resume_command_line`], which builds the line from the original
    /// invocation and is also what refuses a pane that has no command to resume.
    pub launch_command: Option<String>,
}

/// A command that passed every rule, addressed to a pane by id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Approved {
    SendInput {
        pane: PaneId,
        data: Vec<u8>,
    },
    Relaunch {
        pane: PaneId,
        resume_args: Vec<String>,
        /// The line the guard already validated and built. Carried so the
        /// caller runs exactly what was approved instead of rebuilding it.
        command_line: String,
    },
    Status {
        pane: PaneId,
        state: String,
        detail: Option<String>,
        deadline_epoch: Option<i64>,
        attempt: u32,
    },
    /// The plugin may be given this pane. Recording that, and telling the
    /// plugin, is the caller's job.
    WatchPane {
        pane: PaneId,
    },
    Log {
        level: LogLevel,
        message: String,
    },
}

pub struct Guard {
    min_idle: Duration,
    limits: RateLimits,
    budgets: Budgets,
}

impl Guard {
    /// `min_idle` is how long a pane must have been quiet before input may be
    /// typed into it.
    pub fn new(min_idle: Duration, limits: RateLimits) -> Self {
        Self {
            min_idle,
            limits,
            budgets: Budgets::default(),
        }
    }

    /// Forget everything held for the slot `token` names.
    ///
    /// Called when a human types into the pane, when the pane closes, and when
    /// the session is replaced: in all three the plugin's picture of the pane is
    /// void, and its spent budget belongs to a situation that no longer exists.
    /// Keyed by token because the budget is, so this must be called while the
    /// slot still exists — before the slot is retired, not after.
    pub fn cancel(&mut self, token: &PaneToken) {
        self.budgets.clear(token);
    }

    /// Decide one command. Never panics.
    ///
    /// `facts` is what the caller knows about the pane `cmd`'s token resolves
    /// to, or `None` if it resolves to nothing. `allowed_resume_flags` is the
    /// plugin's configured list.
    pub fn judge(
        &mut self,
        cmd: PluginCommand,
        facts: Option<&PaneFacts>,
        allowed_resume_flags: &[String],
        now: Instant,
    ) -> Result<Approved, Refused> {
        match cmd {
            // Not pane-scoped, so none of the pane rules can apply to it.
            PluginCommand::Log { level, message, .. } => Ok(Approved::Log {
                level,
                message: truncate_message(message),
            }),
            PluginCommand::SendInput {
                token,
                generation,
                data,
                ..
            } => {
                let facts = pane_facts(&token, generation, facts)?;
                self.judge_send_input(&token, facts, data, now)
            }
            PluginCommand::Relaunch {
                token,
                generation,
                resume_args,
                ..
            } => {
                let facts = pane_facts(&token, generation, facts)?;
                self.judge_relaunch(&token, facts, resume_args, allowed_resume_flags, now)
            }
            // Deliberately outside `pane_facts`: this is the one command whose
            // whole point is a pane that has *not* opted in, and it names no
            // generation to check.
            PluginCommand::WatchPane { token, .. } => judge_watch(&token, facts),
            PluginCommand::Status {
                token,
                generation,
                state,
                detail,
                deadline_epoch,
                attempt,
                ..
            } => {
                let facts = pane_facts(&token, generation, facts)?;
                // Observability only: nothing happens to the pane, so there is
                // no effect to rate-limit.
                Ok(Approved::Status {
                    pane: facts.pane,
                    state,
                    detail,
                    deadline_epoch,
                    attempt,
                })
            }
        }
    }

    fn judge_send_input(
        &mut self,
        token: &PaneToken,
        facts: &PaneFacts,
        data: String,
        now: Instant,
    ) -> Result<Approved, Refused> {
        let pane = facts.pane;
        if !facts.alive {
            // The slot outlives its process, so typing here would reach
            // whatever occupies it next rather than what the plugin watched.
            return Err(Refused::PaneNotRunning { pane });
        }
        if facts.idle < self.min_idle {
            return Err(Refused::PaneBusy {
                pane,
                idle: facts.idle,
                min_idle: self.min_idle,
            });
        }
        if data.len() > MAX_INPUT_BYTES {
            return Err(Refused::InputTooLarge {
                pane,
                bytes: data.len(),
                limit: MAX_INPUT_BYTES,
            });
        }
        if let Some(bad) = data.chars().find(|c| is_forbidden_control(*c)) {
            return Err(Refused::ControlCharacter {
                pane,
                code: bad as u32,
            });
        }
        self.budgets
            .spend(token, pane, RateAction::SendInput, &self.limits, now)?;
        Ok(Approved::SendInput {
            pane,
            data: data.into_bytes(),
        })
    }

    fn judge_relaunch(
        &mut self,
        token: &PaneToken,
        facts: &PaneFacts,
        resume_args: Vec<String>,
        allowed_resume_flags: &[String],
        now: Instant,
    ) -> Result<Approved, Refused> {
        let pane = facts.pane;
        if facts.alive {
            // Half of the guarantee that one incident is handled once: input
            // recovery acts on live panes, relaunch only on exited ones.
            return Err(Refused::PaneStillRunning { pane });
        }
        if facts.launch_command.is_none() {
            // A bare shell. Putting a process back here would start the shell
            // again, not whatever the person ran inside it, and the resume
            // arguments would have nothing to attach to — so the pane's only
            // recovery is the one typed into it while it is still alive. Checked
            // before `resume_command_line`, which also refuses this, so the log
            // says the pane was never relaunchable rather than blaming the args.
            return Err(Refused::NoLaunchCommand { pane });
        }
        let command_line = resume_command_line(
            facts.launch_command.as_deref(),
            &resume_args,
            allowed_resume_flags,
        )
        .map_err(|e| Refused::ResumeArgsRejected {
            pane,
            reason: e.to_string(),
        })?;
        self.budgets
            .spend(token, pane, RateAction::Relaunch, &self.limits, now)?;
        Ok(Approved::Relaunch {
            pane,
            resume_args,
            command_line,
        })
    }
}

/// Rules 2, 3 and 4: the pane must exist, have opted in, and be the same spawn.
fn pane_facts<'a>(
    token: &PaneToken,
    generation: PaneGeneration,
    facts: Option<&'a PaneFacts>,
) -> Result<&'a PaneFacts, Refused> {
    let Some(facts) = facts else {
        return Err(Refused::UnknownPane {
            token: token.clone(),
        });
    };
    if !facts.opted_in {
        return Err(Refused::NotOptedIn {
            pane: facts.pane,
            token: token.clone(),
        });
    }
    if generation != facts.generation {
        return Err(Refused::StaleGeneration {
            pane: facts.pane,
            claimed: generation,
            current: facts.generation,
        });
    }
    Ok(facts)
}

#[cfg(test)]
#[path = "guard_tests/mod.rs"]
mod tests;
