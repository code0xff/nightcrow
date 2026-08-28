//! What every provider adapter must be able to answer.
//!
//! The state machine knows nothing about any particular CLI: it asks an adapter
//! "did this pane just hit a usage limit, and when does that limit reset", and
//! later "how do I get this session going again". Everything provider-specific —
//! which hook fires, which file to tail, which flag resumes a session — lives in
//! one file per adapter.

use crate::protocol::{PaneGeneration, PaneToken};
use serde_json::Value;
use std::path::Path;

pub mod claude;
pub mod codex;
pub mod opencode;

/// Which pane an adapter is being asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneContext {
    pub token: PaneToken,
    pub generation: PaneGeneration,
    pub cwd: String,
    pub command: Option<String>,
}

/// Why a provider stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// A plan/usage limit that clears at a known or estimated time. Worth
    /// waiting for.
    UsageLimit,
    /// An overload or server error. Worth a short backoff, not a long wait.
    Transient,
    /// Auth, billing, or a bad request. No amount of waiting fixes it, so the
    /// machine stops and says so instead of retrying.
    NeedsHuman,
}

/// An adapter's report that a pane's provider stopped for a limit-like reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitEvent {
    /// The provider's own session identifier, when it is known exactly. Without
    /// it a resume can only be refused: resuming "the last session" could pick
    /// up a different pane's work.
    pub session_id: Option<String>,
    /// Absolute unix seconds at which the limit clears, when the provider said
    /// so. `None` means the machine must fall back to bounded backoff.
    pub resets_at: Option<i64>,
    pub kind: LimitKind,
    /// Short, non-sensitive explanation for the host's status line. Never
    /// carries transcript text or a raw payload.
    pub detail: String,
}

impl LimitEvent {
    pub fn usage(session_id: Option<String>, resets_at: Option<i64>, detail: &str) -> Self {
        Self {
            session_id,
            resets_at,
            kind: LimitKind::UsageLimit,
            detail: detail.to_string(),
        }
    }
}

/// A signal that did not come through the pane's terminal output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalKind {
    /// Claude Code's `StopFailure` hook payload.
    StopFailure,
    /// The `rate_limits` object from Claude Code's statusline payload.
    RateLimits,
    /// Claude Code's `Stop` hook: a turn ended, however it ended. Carries no
    /// payload and never reaches a provider, because wanting the person back
    /// is not a provider question.
    TurnEnd,
}

impl SignalKind {
    /// The wire name used on the IPC socket. Anything else is rejected there.
    pub fn as_wire(&self) -> &'static str {
        match self {
            Self::StopFailure => "stop_failure",
            Self::RateLimits => "rate_limits",
            Self::TurnEnd => "turn_end",
        }
    }

    pub fn from_wire(s: &str) -> Option<Self> {
        match s {
            "stop_failure" => Some(Self::StopFailure),
            "rate_limits" => Some(Self::RateLimits),
            "turn_end" => Some(Self::TurnEnd),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfBand {
    pub kind: SignalKind,
    pub payload: Value,
}

/// How to get a stopped session going again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumePlan {
    /// Append these args to the pane's original command. The host supplies the
    /// program, so this can never name a different binary.
    Relaunch(Vec<String>),
    /// Type this into a pane whose process is still alive and idle.
    Input(String),
    /// The adapter knows this pane must not be touched yet, and why.
    Hold(&'static str),
}

pub trait Provider {
    /// Stable adapter name, reported in `status` and used in logs.
    fn name(&self) -> &'static str;

    /// Terminal text the pane produced. This is the least reliable source —
    /// wording changes between releases — so an adapter uses it only as a
    /// documented fallback.
    fn on_output(
        &mut self,
        _ctx: &PaneContext,
        _text: &str,
        _now_epoch: i64,
    ) -> Option<LimitEvent> {
        None
    }

    /// A signal that arrived over the IPC socket.
    fn on_signal(
        &mut self,
        _ctx: &PaneContext,
        _signal: &OutOfBand,
        _now_epoch: i64,
    ) -> Option<LimitEvent> {
        None
    }

    /// Called on the plugin's timer, for an adapter that has to look somewhere
    /// itself (a rollout file, an HTTP endpoint).
    fn poll(&mut self, _ctx: &PaneContext, _now_epoch: i64) -> Option<LimitEvent> {
        None
    }

    /// The pane's process has ended. Adapters that must not act while a
    /// provider is retrying internally use this as their gate.
    fn on_exit(&mut self, _ctx: &PaneContext) {}

    /// How to resume, or `None` when this adapter cannot say safely.
    ///
    /// `alive` is the host's word that the pane's process is still running; an
    /// adapter must only answer [`ResumePlan::Input`] when it is true.
    fn resume(&self, ctx: &PaneContext, limit: &LimitEvent, alive: bool) -> Option<ResumePlan>;
}

/// Pick an adapter from the pane's command line, or `None` when the pane runs
/// something this plugin knows nothing about — in which case the plugin stays
/// out of the pane entirely.
pub fn detect(command: Option<&str>) -> Option<Box<dyn Provider>> {
    let word = first_word(command?)?;
    let program = Path::new(word).file_name()?.to_str()?;
    #[cfg(windows)]
    let program = program.to_ascii_lowercase();
    #[cfg(windows)]
    let program = [".exe", ".cmd", ".bat", ".com", ".ps1"]
        .iter()
        .find_map(|suffix| program.strip_suffix(suffix))
        .unwrap_or(&program);
    match program {
        "claude" => Some(Box::new(claude::Claude::default())),
        "codex" => Some(Box::new(codex::Codex::default())),
        "opencode" => Some(Box::new(opencode::OpenCode::default())),
        _ => None,
    }
}

/// Pick an adapter from a signal that arrived over the IPC socket, for a pane
/// whose command line says nothing — the shell somebody opened and then
/// started a provider CLI inside by hand.
///
/// Sound because a [`SignalKind`] is minted by exactly one provider's helper:
/// a `stop_failure` line can only have come from the Claude Code hook this
/// binary installed. The signal is therefore evidence of what the pane is
/// running, in a way terminal text never is — which is why this is a lookup on
/// the wire kind and deliberately not a second sniffing path. A kind added
/// later has to be classified here rather than falling through to a guess.
pub fn detect_from_signal(kind: SignalKind) -> Option<Box<dyn Provider>> {
    match kind {
        SignalKind::StopFailure | SignalKind::RateLimits | SignalKind::TurnEnd => {
            Some(Box::new(claude::Claude::default()))
        }
    }
}

/// The command's first shell word, including a quoted executable path.
fn first_word(command: &str) -> Option<&str> {
    let command = command.trim_start();
    let quote = command.chars().next()?;
    if quote == '"' || (cfg!(not(windows)) && quote == '\'') {
        let quoted = &command[quote.len_utf8()..];
        let end = quoted.find(quote)?;
        let trailing = &quoted[end + quote.len_utf8()..];
        if !trailing.is_empty() && !trailing.starts_with(char::is_whitespace) {
            return None;
        }
        return (!quoted[..end].is_empty()).then_some(&quoted[..end]);
    }

    command.split_whitespace().next()
}

/// Furthest ahead a reported reset time is believed.
///
/// Claude's longest documented window is seven days, so anything beyond eight is
/// either a different unit or a corrupt value; treating it as absent keeps a
/// bogus number from parking a pane for months.
pub const MAX_RESET_HORIZON_SECS: i64 = 8 * 24 * 60 * 60;

/// Earliest plausible unix second for a reset time (2020-01-01). Below this a
/// value is a duration, a millisecond count that lost precision, or garbage.
const MIN_PLAUSIBLE_EPOCH_SECS: i64 = 1_577_836_800;

/// Read a reset time from provider JSON as absolute unix seconds.
///
/// Returns `None` for a missing field, a non-number, a non-integer, a value
/// outside the plausible band, and a time already far in the past — every one of
/// which must degrade to bounded backoff rather than to a wait of the wrong
/// length.
pub fn reset_epoch_from_json(payload: &Value, path: &[&str], now_epoch: i64) -> Option<i64> {
    let mut node = payload;
    for key in path {
        node = node.get(key)?;
    }
    let secs = node.as_i64()?;
    plausible_reset(secs, now_epoch)
}

/// Accept a reset time only if it could really be one.
pub fn plausible_reset(secs: i64, now_epoch: i64) -> Option<i64> {
    if secs < MIN_PLAUSIBLE_EPOCH_SECS {
        return None;
    }
    if secs > now_epoch.saturating_add(MAX_RESET_HORIZON_SECS) {
        return None;
    }
    Some(secs)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
