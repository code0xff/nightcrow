//! Claude Code adapter.
//!
//! Three sources, in descending order of trust: the `StopFailure` hook (says
//! exactly why a turn ended), the statusline's `rate_limits` object (says when a
//! window resets, but never that we are blocked), and pane text (a fallback for
//! users who have neither, i.e. no hook installed and no Pro/Max subscription).

use super::{
    LimitEvent, LimitKind, OutOfBand, PaneContext, Provider, ResumePlan, SignalKind,
    reset_epoch_from_json,
};
use crate::protocol::PaneGeneration;
use serde_json::Value;

/// Hook name we require when the payload names itself, so a mislabelled or
/// replayed payload cannot be read as a stop failure.
const STOP_FAILURE_EVENT: &str = "StopFailure";

/// Windows the statusline may report. Each is independently optional: the object
/// only exists for Pro/Max accounts, and only after the session's first response.
const RATE_LIMIT_WINDOWS: &[&str] = &["five_hour", "seven_day"];

/// A session id is a UUID (36 chars); the cap is deliberately loose but finite so
/// an over-long value is rejected rather than handed to a command line.
const MAX_SESSION_ID_BYTES: usize = 64;

/// How much recent output is kept so a needle split across two `pane_output`
/// events is still found. One screenful of a wide terminal is a few KiB; 4 KiB
/// spans that without holding transcript-sized history in memory.
const OUTPUT_TAIL_BYTES: usize = 4 * 1024;

/// Phrasings that unambiguously mean the account is blocked by usage. Compared
/// against lowercased text. Kept narrow on purpose: a false positive parks a
/// working pane, and the hook already covers the common case.
const LIMIT_NEEDLES: &[&str] = &[
    "usage limit reached",
    // Covers both the ASCII and the typographic apostrophe in "you've".
    "hit your usage limit",
];

/// Phrasings that look similar but only warn. Checked before [`LIMIT_NEEDLES`];
/// suppressing is the safe direction, because a missed limit still reaches the
/// machine through the hook or the next output chunk.
const NOT_A_LIMIT_NEEDLES: &[&str] = &[
    "approaching your usage limit",
    "approaching the usage limit",
];

/// Typed into a live pane. Claude Code keeps running after an API error, so the
/// recovery is a nudge, not a relaunch. A plain continuation word only — never a
/// flag, never a permission grant.
const NUDGE_INPUT: &str = "continue\r";

/// Flag that resumes a named session; the id follows as a positional argument.
const RESUME_FLAG: &str = "--resume";

const NEEDS_HUMAN_HOLD: &str =
    "claude reported an auth or billing failure; waiting cannot clear it";
const NO_SESSION_HOLD: &str =
    "no Claude session id; resuming the wrong session is worse than stopping";
const OUTPUT_FALLBACK_DETAIL: &str = "claude output says the usage limit is reached";

/// Adapter state for one pane. Nothing here is written to disk, and neither
/// `error_message` nor transcript text is ever retained.
#[derive(Debug, Default)]
pub struct Claude {
    /// Generation the rest of this state belongs to; a change re-arms the latch.
    generation: Option<PaneGeneration>,
    /// Earliest plausible reset time the statusline has reported.
    resets_at: Option<i64>,
    /// Last validated session id seen on a hook payload.
    session_id: Option<String>,
    /// Lowercased tail of recent output, at most [`OUTPUT_TAIL_BYTES`].
    tail: String,
    /// Whether the output fallback has already fired for this generation.
    fired: bool,
}

impl Claude {
    fn sync_generation(&mut self, ctx: &PaneContext) {
        if self.generation == Some(ctx.generation) {
            return;
        }
        self.generation = Some(ctx.generation);
        self.fired = false;
        self.tail.clear();
        // A respawn is a different session, so the old id must not be reused.
        // The reset time is an account-wide fact and survives the respawn.
        self.session_id = None;
    }

    fn remember_reset(&mut self, at: i64) {
        // The earliest window to reopen is the one that decides when work can
        // continue, so the minimum is the useful deadline.
        self.resets_at = Some(match self.resets_at {
            Some(known) => known.min(at),
            None => at,
        });
    }

    fn push_tail(&mut self, text: &str) {
        self.tail.push_str(&text.to_lowercase());
        if self.tail.len() <= OUTPUT_TAIL_BYTES {
            return;
        }
        let want = self.tail.len() - OUTPUT_TAIL_BYTES;
        let cut = (want..=self.tail.len())
            .find(|i| self.tail.is_char_boundary(*i))
            .unwrap_or(self.tail.len());
        self.tail.drain(..cut);
    }

    fn on_rate_limits(&mut self, payload: &Value, now_epoch: i64) {
        for window in RATE_LIMIT_WINDOWS {
            if let Some(at) = reset_epoch_from_json(payload, &[window, "resets_at"], now_epoch) {
                self.remember_reset(at);
            }
        }
        // `used_percentage` is deliberately ignored, including at 100: a full
        // window corroborates a limit but does not declare one, and only
        // StopFailure or the output fallback may declare.
    }

    fn on_stop_failure(&mut self, payload: &Value) -> Option<LimitEvent> {
        let named = payload.get("hook_event_name").and_then(Value::as_str);
        if named.is_some_and(|name| name != STOP_FAILURE_EVENT) {
            return None;
        }
        let error_type = payload.get("error_type").and_then(Value::as_str)?;
        let (kind, detail) = classify(error_type)?;
        if let Some(id) = payload
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(validated_session_id)
        {
            self.session_id = Some(id);
        }
        Some(LimitEvent {
            session_id: self.session_id.clone(),
            resets_at: self.resets_at,
            kind,
            detail: detail.to_string(),
        })
    }
}

impl Provider for Claude {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn on_signal(
        &mut self,
        ctx: &PaneContext,
        signal: &OutOfBand,
        now_epoch: i64,
    ) -> Option<LimitEvent> {
        self.sync_generation(ctx);
        match signal.kind {
            SignalKind::RateLimits => {
                self.on_rate_limits(&signal.payload, now_epoch);
                None
            }
            SignalKind::StopFailure => self.on_stop_failure(&signal.payload),
            // Intercepted before a provider ever sees it: a turn ending is a
            // fact about the person's attention, not about usage limits.
            SignalKind::TurnEnd => None,
        }
    }

    fn on_output(&mut self, ctx: &PaneContext, text: &str, _now_epoch: i64) -> Option<LimitEvent> {
        self.sync_generation(ctx);
        self.push_tail(text);
        if self.fired {
            return None;
        }
        if NOT_A_LIMIT_NEEDLES.iter().any(|n| self.tail.contains(n)) {
            return None;
        }
        if !LIMIT_NEEDLES.iter().any(|n| self.tail.contains(n)) {
            return None;
        }
        // A TUI redraws the same line many times, so latch and drop the matched
        // text instead of reporting it again on the next repaint.
        self.fired = true;
        self.tail.clear();
        // No wall-clock time is parsed out of the text: it is printed without an
        // offset, so a deadline read from it would be ambiguous.
        Some(LimitEvent::usage(
            self.session_id.clone(),
            self.resets_at,
            OUTPUT_FALLBACK_DETAIL,
        ))
    }

    fn on_exit(&mut self, _ctx: &PaneContext) {
        self.fired = false;
        self.tail.clear();
    }

    fn resume(&self, _ctx: &PaneContext, limit: &LimitEvent, alive: bool) -> Option<ResumePlan> {
        if limit.kind == LimitKind::NeedsHuman {
            return Some(ResumePlan::Hold(NEEDS_HUMAN_HOLD));
        }
        if alive {
            return Some(ResumePlan::Input(NUDGE_INPUT.to_string()));
        }
        let Some(id) = limit.session_id.as_deref().and_then(validated_session_id) else {
            return Some(ResumePlan::Hold(NO_SESSION_HOLD));
        };
        Some(ResumePlan::Relaunch(vec![RESUME_FLAG.to_string(), id]))
    }
}

/// Map a hook `error_type` to a kind and a fixed detail string.
///
/// `None` means "not a limit, nothing to recover". The detail is a literal, not
/// the payload's `error_message`, which can carry account and quota text.
fn classify(error_type: &str) -> Option<(LimitKind, &'static str)> {
    match error_type {
        "rate_limit" => Some((LimitKind::UsageLimit, "claude api error: rate_limit")),
        "overloaded" => Some((LimitKind::Transient, "claude api error: overloaded")),
        "server_error" => Some((LimitKind::Transient, "claude api error: server_error")),
        "authentication_failed" => Some((
            LimitKind::NeedsHuman,
            "claude api error: authentication_failed",
        )),
        "oauth_org_not_allowed" => Some((
            LimitKind::NeedsHuman,
            "claude api error: oauth_org_not_allowed",
        )),
        "billing_error" => Some((LimitKind::NeedsHuman, "claude api error: billing_error")),
        _ => None,
    }
}

/// Accept a session id only if it is safe to hand back as a command-line
/// argument: non-empty, bounded, and made of ASCII alphanumerics, `-`, or `_`.
fn validated_session_id(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.len() > MAX_SESSION_ID_BYTES {
        return None;
    }
    if !raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(raw.to_string())
}

#[cfg(test)]
#[path = "claude_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "claude_output_tests.rs"]
mod output_tests;
