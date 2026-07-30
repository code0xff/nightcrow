//! Turning "the wait is over" into a single request to the host.
//!
//! Split out of `state.rs` to keep each file readable: `state.rs` owns time and
//! transitions, this owns the one moment the plugin actually asks for something.
//!
//! Everything here is written on the assumption that the host will refuse. A
//! refusal costs an attempt and nothing else, so the checks below exist to avoid
//! wasting attempts on requests that are obviously going to be rejected — not to
//! be the safety boundary. That boundary is the host's.

use super::{MAX_RESUME_ATTEMPTS, PaneRecovery, RecoveryState};
use crate::protocol::{MAX_INPUT_BYTES, PROTOCOL_VERSION, PluginCommand};
use crate::provider::{PaneContext, Provider, ResumePlan};
use std::time::Instant;

/// Most arguments a resume may consist of, matching the host's own cap: a resume
/// is a flag and an identifier, and anything longer is not a resume.
const MAX_RESUME_ARGS: usize = 6;

/// Longest single resume argument, matching the host's cap. Past a UUID or a
/// session name with room to spare.
const MAX_RESUME_ARG_LEN: usize = 256;

/// Characters a resume argument may consist of, matching the host's rule.
///
/// The host appends the argument to a command line a login shell parses, so it
/// refuses anything carrying a space, quote, backtick, `$` or `;`. Checking the
/// same rule here means a session id picked up from a provider's file cannot
/// spend an attempt on a request that was never going to be accepted.
fn is_safe_arg_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/' | '=' | '@' | '+')
}

fn args_are_safe(args: &[String]) -> bool {
    !args.is_empty()
        && args.len() <= MAX_RESUME_ARGS
        && args.iter().all(|a| {
            !a.is_empty() && a.len() <= MAX_RESUME_ARG_LEN && a.chars().all(is_safe_arg_char)
        })
}

impl PaneRecovery {
    /// Ask the adapter how to resume and, if the pane is in a state we may
    /// touch, send exactly one request.
    ///
    /// Staying in [`RecoveryState::ReadyToResume`] with nothing emitted is the
    /// normal answer while the pane is not yet touchable: a relaunch needs the
    /// process gone, typed input needs it alive *and* idle, and both of those
    /// facts arrive as later host events.
    pub(super) fn try_resume(
        &mut self,
        provider: &dyn Provider,
        ctx: &PaneContext,
        now_epoch: i64,
        now: Instant,
    ) -> Vec<PluginCommand> {
        let Some(limit) = self.limit.clone() else {
            // Reaching a resume with no episode means the episode was cancelled
            // between the wait ending and this tick. Nothing to do.
            return self.cancel();
        };
        let Some(plan) = provider.resume(ctx, &limit, self.alive) else {
            return self.arm_wait_after_failure(now_epoch, now);
        };
        match plan {
            ResumePlan::Hold(reason) => {
                self.detail = Some(reason.to_string());
                self.goto(RecoveryState::NeedsAttention)
            }
            ResumePlan::Input(data) => self.send_input(data, now_epoch, now),
            ResumePlan::Relaunch(args) => self.relaunch(args, now),
        }
    }

    fn send_input(&mut self, data: String, now_epoch: i64, now: Instant) -> Vec<PluginCommand> {
        if !self.alive {
            // The adapter offered typed input for a process that has since
            // exited. Do not invent a relaunch on its behalf.
            return self.arm_wait_after_failure(now_epoch, now);
        }
        if !self.idle {
            return Vec::new();
        }
        if data.is_empty() || data.len() > MAX_INPUT_BYTES {
            self.detail = Some("adapter offered input the host would refuse".to_string());
            return self.goto(RecoveryState::NeedsAttention);
        }
        let command = PluginCommand::SendInput {
            v: PROTOCOL_VERSION,
            token: self.token.clone(),
            generation: self.generation,
            data,
        };
        self.spend_attempt(command, now)
    }

    fn relaunch(&mut self, args: Vec<String>, now: Instant) -> Vec<PluginCommand> {
        if self.alive {
            // The host refuses a relaunch of a live pane, so wait for the exit
            // rather than spending an attempt learning that again.
            return Vec::new();
        }
        if !args_are_safe(&args) {
            self.detail = Some("adapter offered resume args the host would refuse".to_string());
            return self.goto(RecoveryState::NeedsAttention);
        }
        let command = PluginCommand::Relaunch {
            v: PROTOCOL_VERSION,
            token: self.token.clone(),
            generation: self.generation,
            resume_args: args,
        };
        self.spend_attempt(command, now)
    }

    /// Count the attempt, emit the request, and start the confirmation timer.
    fn spend_attempt(&mut self, command: PluginCommand, now: Instant) -> Vec<PluginCommand> {
        if self.attempt >= MAX_RESUME_ATTEMPTS {
            self.detail = Some(format!(
                "gave up after {MAX_RESUME_ATTEMPTS} resume attempts"
            ));
            return self.goto(RecoveryState::NeedsAttention);
        }
        self.attempt += 1;
        self.resumed_at = Some(now);
        let mut out = self.goto(RecoveryState::Resuming);
        out.push(command);
        out
    }
}
