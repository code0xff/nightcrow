//! Waiting: the half of the machine driven by the clock rather than by an event.
//!
//! Split out of `state.rs` for readability. Nothing here decides *what* to do
//! about a limit; it decides only how long to sit still first, and it is the
//! one place that can end a recovery by running out of attempts.

use super::{MAX_RESUME_ATTEMPTS, PaneRecovery, RESUME_CONFIRM_SECS, RecoveryState};
use crate::protocol::PluginCommand;
use crate::provider::{LimitKind, PaneContext, Provider};
use crate::wait::ResetWait;
use std::time::{Duration, Instant};

impl PaneRecovery {
    /// Advance anything that is only a function of time.
    pub fn tick(
        &mut self,
        provider: &dyn Provider,
        ctx: &PaneContext,
        now_epoch: i64,
        now: Instant,
    ) -> Vec<PluginCommand> {
        let mut out = Vec::new();
        match self.state {
            RecoveryState::WaitingForReset | RecoveryState::Backoff => {
                let elapsed = self.wait.as_mut().is_some_and(|w| w.poll(now_epoch, now));
                if elapsed {
                    self.wait = None;
                    out.extend(self.goto(RecoveryState::ReadyToResume));
                }
            }
            RecoveryState::Resuming => {
                let stale = self.resumed_at.is_some_and(|at| {
                    now.duration_since(at) >= Duration::from_secs(RESUME_CONFIRM_SECS)
                });
                if stale {
                    self.resumed_at = None;
                    out.extend(self.arm_wait_after_failure(now_epoch, now));
                }
            }
            _ => {}
        }
        if self.state == RecoveryState::ReadyToResume {
            out.extend(self.try_resume(provider, ctx, now_epoch, now));
        }
        out
    }

    pub(super) fn arm_wait(&mut self, now_epoch: i64, now: Instant) -> Vec<PluginCommand> {
        // A known reset time is waited out exactly once and does not spend an
        // attempt: nothing has been tried yet.
        let reset = self
            .limit
            .as_ref()
            .filter(|l| l.kind == LimitKind::UsageLimit)
            .and_then(|l| l.resets_at);
        if let Some(reset) = reset {
            self.wait = Some(ResetWait::until(reset, now_epoch, now));
            return self.goto(RecoveryState::WaitingForReset);
        }
        self.arm_backoff(now_epoch, now)
    }

    /// Re-arm after a resume that did not land. Distinct from [`Self::arm_wait`]
    /// because a failed resume must never go back to waiting on the same reset
    /// time — that time has passed.
    pub(super) fn arm_wait_after_failure(
        &mut self,
        now_epoch: i64,
        now: Instant,
    ) -> Vec<PluginCommand> {
        self.detail = Some("resume produced no sign of life".to_string());
        self.arm_backoff(now_epoch, now)
    }

    fn arm_backoff(&mut self, now_epoch: i64, now: Instant) -> Vec<PluginCommand> {
        if self.attempt >= MAX_RESUME_ATTEMPTS {
            self.wait = None;
            self.detail = Some(format!(
                "gave up after {MAX_RESUME_ATTEMPTS} resume attempts"
            ));
            return self.goto(RecoveryState::NeedsAttention);
        }
        self.wait = Some(ResetWait::backoff(self.attempt + 1, now_epoch, now));
        self.goto(RecoveryState::Backoff)
    }
}
