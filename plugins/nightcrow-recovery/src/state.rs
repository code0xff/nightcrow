//! The recovery state machine for one pane slot.
//!
//! Deliberately provider-agnostic: an adapter says "this pane hit a limit and it
//! clears at T" and "here is how to resume", and everything about *when* to act,
//! how many times, and when to stop lives here. The machine holds no IO and no
//! clock of its own — every entry point takes the current time — so the awkward
//! cases (a stale generation, a clock jump, an exhausted attempt budget) are
//! ordinary unit tests rather than something only reproducible by waiting.
//!
//! Safety posture: this machine never decides that a pane is alive or idle; it
//! only repeats back what the host told it, and refuses to ask for input unless
//! the host has said both. The host judges every request again anyway.

use crate::protocol::{PROTOCOL_VERSION, PaneGeneration, PaneToken, PluginCommand, PluginEvent};
use crate::provider::{LimitEvent, LimitKind};
use crate::wait::ResetWait;
use std::time::Instant;

/// Resume attempts allowed per limit episode before the pane is handed back to
/// its human.
///
/// Four is enough to ride out a reset time that was slightly optimistic plus a
/// couple of transient failures. Beyond that the cause is not something waiting
/// fixes, and a fifth automatic attempt is just noise in someone's terminal.
pub const MAX_RESUME_ATTEMPTS: u32 = 4;

/// How long a resume has to show some sign of life before it is treated as
/// failed.
///
/// A relaunch reports back as a new generation within milliseconds and typed
/// input echoes almost as fast, so this only has to cover a slow provider
/// start-up; a minute and a half is generous and still bounded.
pub const RESUME_CONFIRM_SECS: u64 = 90;

/// Where a pane is in its recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryState {
    /// Nothing to do. The steady state, and where every cancellation lands.
    Idle,
    /// A limit was reported; the machine has not yet decided how to wait.
    LimitDetected,
    /// Waiting for a known reset time.
    WaitingForReset,
    /// The wait is over; waiting on the pane to be in a state we may touch.
    ReadyToResume,
    /// A resume was asked of the host; waiting for a sign it landed.
    Resuming,
    /// No reset time, or a resume that did not land: waiting out a backoff step.
    Backoff,
    /// Given up. Only a human clears this.
    NeedsAttention,
}

impl RecoveryState {
    /// The name the host displays. Stable: it is part of what a user reads.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::LimitDetected => "limit_detected",
            Self::WaitingForReset => "waiting_for_reset",
            Self::ReadyToResume => "ready_to_resume",
            Self::Resuming => "resuming",
            Self::Backoff => "backoff",
            Self::NeedsAttention => "needs_attention",
        }
    }
}

/// One pane slot's recovery progress.
#[derive(Debug)]
pub struct PaneRecovery {
    token: PaneToken,
    generation: PaneGeneration,
    state: RecoveryState,
    limit: Option<LimitEvent>,
    wait: Option<ResetWait>,
    attempt: u32,
    /// The host's word on the pane's process, never this machine's guess.
    alive: bool,
    idle: bool,
    detail: Option<String>,
    resumed_at: Option<Instant>,
}

impl PaneRecovery {
    pub fn new(token: PaneToken, generation: PaneGeneration) -> Self {
        Self {
            token,
            generation,
            state: RecoveryState::Idle,
            limit: None,
            wait: None,
            attempt: 0,
            alive: true,
            idle: false,
            detail: None,
            resumed_at: None,
        }
    }

    pub fn state(&self) -> RecoveryState {
        self.state
    }

    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn generation(&self) -> PaneGeneration {
        self.generation
    }

    #[cfg(test)]
    pub fn session_id(&self) -> Option<&str> {
        self.limit.as_ref()?.session_id.as_deref()
    }

    pub fn deadline_epoch(&self) -> Option<i64> {
        self.wait.as_ref().map(ResetWait::deadline_epoch)
    }

    #[cfg(test)]
    pub fn alive(&self) -> bool {
        self.alive
    }

    /// Fold in one host event.
    ///
    /// Returns `None` when the event names a generation this machine has already
    /// moved past — a decision about a dead process must never land on its
    /// successor — and otherwise the commands the transition produced.
    pub fn on_event(&mut self, event: &PluginEvent) -> Option<Vec<PluginCommand>> {
        let generation = event.generation()?;
        if generation < self.generation {
            return None;
        }
        let mut out = Vec::new();
        if generation > self.generation {
            // A new spawn of the slot voids everything decided about the
            // previous process. A relaunch we asked for counts as a resume
            // that worked; a respawn we did not ask for is a plain cancellation.
            // Either way the machine lands in `Idle`.
            self.generation = generation;
            self.alive = true;
            self.idle = false;
            out.extend(if self.state == RecoveryState::Resuming {
                self.confirm_resume()
            } else {
                self.cancel()
            });
        }
        match event {
            PluginEvent::PaneOpened { .. } => {
                self.alive = true;
                self.idle = false;
            }
            PluginEvent::PaneOutput { .. } => {
                self.idle = false;
                out.extend(self.confirm_resume());
            }
            PluginEvent::PaneIdle { .. } => {
                self.idle = true;
                out.extend(self.confirm_resume());
            }
            PluginEvent::PaneExited { .. } => {
                self.alive = false;
                self.idle = false;
            }
            PluginEvent::PaneClosed { .. } | PluginEvent::UserInput { .. } => {
                // The slot is gone, or its human took it back. Either way this
                // machine has no business acting on it again; the attempt
                // budget resets because the next episode is a fresh one.
                self.attempt = 0;
                out.extend(self.cancel());
            }
            PluginEvent::Shutdown { .. } => {}
        }
        Some(out)
    }

    /// Record an adapter's limit report. Idempotent: the same episode reported
    /// twice changes nothing.
    pub fn note_limit(
        &mut self,
        limit: LimitEvent,
        now_epoch: i64,
        now: Instant,
    ) -> Vec<PluginCommand> {
        if self.state == RecoveryState::NeedsAttention || self.is_same_episode(&limit) {
            return Vec::new();
        }
        self.detail = Some(limit.detail.clone());
        let kind = limit.kind;
        self.limit = Some(limit);
        let mut out = self.goto(RecoveryState::LimitDetected);
        if kind == LimitKind::NeedsHuman {
            out.extend(self.goto(RecoveryState::NeedsAttention));
            return out;
        }
        out.extend(self.arm_wait(now_epoch, now));
        out
    }

    /// A limit episode is the triple (token, generation, provider session id)
    /// plus the reset time. Anything else is a new episode and re-arms the wait.
    fn is_same_episode(&self, limit: &LimitEvent) -> bool {
        let Some(current) = &self.limit else {
            return false;
        };
        self.state != RecoveryState::Idle
            && current.session_id == limit.session_id
            && current.resets_at == limit.resets_at
            && current.kind == limit.kind
    }

    /// A sign that a resume landed. Only meaningful while [`RecoveryState::Resuming`].
    fn confirm_resume(&mut self) -> Vec<PluginCommand> {
        if self.state != RecoveryState::Resuming {
            return Vec::new();
        }
        // The attempt budget is refunded only for an episode that had a real
        // reset time to wait for — those are bounded by the provider's own
        // window, so refunding cannot spin. An episode with no known reset
        // time keeps its count, which is what stops a pane that resumes
        // cleanly and then immediately fails again from retrying forever.
        if self.limit.as_ref().and_then(|l| l.resets_at).is_some() {
            self.attempt = 0;
        }
        self.resumed_at = None;
        self.limit = None;
        self.wait = None;
        self.detail = Some("resumed".to_string());
        self.goto(RecoveryState::Idle)
    }

    fn cancel(&mut self) -> Vec<PluginCommand> {
        self.wait = None;
        self.limit = None;
        self.resumed_at = None;
        if self.state == RecoveryState::Idle {
            return Vec::new();
        }
        self.detail = Some("cancelled".to_string());
        self.goto(RecoveryState::Idle)
    }

    fn goto(&mut self, state: RecoveryState) -> Vec<PluginCommand> {
        if self.state == state {
            return Vec::new();
        }
        self.state = state;

        vec![self.status()]
    }

    fn status(&self) -> PluginCommand {
        PluginCommand::Status {
            v: PROTOCOL_VERSION,
            token: self.token.clone(),
            generation: self.generation,
            state: self.state.as_str().to_string(),
            detail: self.detail.clone(),
            deadline_epoch: self.deadline_epoch(),
            attempt: self.attempt,
        }
    }
}

#[path = "state_clock.rs"]
mod clock;

#[path = "state_resume.rs"]
mod resume;

#[cfg(test)]
#[path = "state_tests/mod.rs"]
mod tests;
