//! The two modes a provider CLI invokes, not a human.
//!
//! Both run inside a child of the provider's own process, on that process's
//! critical path: Claude Code runs the statusline command on a refresh interval
//! and the hook command as a turn ends. So both do the least possible work —
//! read one JSON object, forward a handful of whitelisted fields, exit — and
//! neither ever reports a failure to its caller. A recovery plugin that is not
//! running must look exactly like one that was never installed.
//!
//! Whitelisting is the privacy boundary. A `StopFailure` payload names a
//! transcript file and carries a provider's own error prose; a statusline payload
//! carries whatever else the provider decided to include. Only the fields the
//! state machine actually reads cross the socket, so nothing else can be
//! accidentally logged, buffered, or written down later.

use crate::ipc::{IpcMessage, send, socket_path};
use crate::protocol::PANE_TOKEN_ENV;
use crate::provider::SignalKind;
use serde_json::{Map, Value};
use std::io::Read;
use std::process::ExitCode;

/// Most stdin a helper will read.
///
/// A hook payload is a handful of short strings plus an error message; 64 KiB is
/// far past any of that and keeps a provider that streams into us from making
/// this process grow.
const MAX_HELPER_STDIN_BYTES: u64 = 64 * 1024;

/// Fields of a `StopFailure` payload the state machine reads. Everything else —
/// `error_message`, `transcript_path`, `prompt_id`, `cwd` — stays in the
/// provider's process.
const STOP_FAILURE_FIELDS: [&str; 3] = ["session_id", "error_type", "hook_event_name"];

/// The only statusline field this plugin wants.
const RATE_LIMITS_FIELD: &str = "rate_limits";

/// Shown when the statusline payload carries no usage numbers — which is normal:
/// `rate_limits` is absent for accounts without a subscription window and before
/// the session's first response.
const STATUSLINE_FALLBACK: &str = "nightcrow: watching";

/// Forward a `StopFailure` payload. Always succeeds from the caller's point of
/// view; `StopFailure` ignores our exit code anyway, and a hook that fails
/// loudly would be worse than one that does nothing.
pub fn hook() -> ExitCode {
    if let Some((token, payload)) = read_stdin_object().and_then(|body| {
        let token = pane_token()?;
        Some((token, pick(&body, &STOP_FAILURE_FIELDS)))
    }) {
        let _ = send(
            &socket_path().unwrap_or_default(),
            &IpcMessage {
                token,
                kind: SignalKind::StopFailure,
                payload: Value::Object(payload),
            },
        );
    }
    ExitCode::SUCCESS
}

/// Forward the statusline's `rate_limits` and print a line, so installing this
/// plugin does not cost the user their statusline.
pub fn statusline() -> ExitCode {
    let body = read_stdin_object();
    let rate_limits = body
        .as_ref()
        .and_then(|b| b.get(RATE_LIMITS_FIELD))
        .and_then(Value::as_object)
        .cloned();
    if let (Some(token), Some(limits)) = (pane_token(), rate_limits.clone()) {
        let _ = send(
            &socket_path().unwrap_or_default(),
            &IpcMessage {
                token,
                kind: SignalKind::RateLimits,
                payload: Value::Object(limits),
            },
        );
    }
    println!("{}", render_statusline(rate_limits.as_ref()));
    ExitCode::SUCCESS
}

/// A short line built only from fields whose meaning is documented: the usage
/// percentage of each window the provider reported.
fn render_statusline(rate_limits: Option<&Map<String, Value>>) -> String {
    let Some(limits) = rate_limits else {
        return STATUSLINE_FALLBACK.to_string();
    };
    let mut parts = Vec::new();
    for (label, key) in [("5h", "five_hour"), ("7d", "seven_day")] {
        if let Some(used) = limits
            .get(key)
            .and_then(|w| w.get("used_percentage"))
            .and_then(Value::as_f64)
        {
            parts.push(format!("{label} {}%", used.round() as i64));
        }
    }
    if parts.is_empty() {
        return STATUSLINE_FALLBACK.to_string();
    }
    parts.join(" | ")
}

/// The pane this helper belongs to, from the environment its provider inherited.
/// Absent means this provider was not started by nightcrow, so there is nothing
/// to correlate and nothing to send.
fn pane_token() -> Option<String> {
    std::env::var(PANE_TOKEN_ENV)
        .ok()
        .filter(|t| !t.trim().is_empty())
}

fn read_stdin_object() -> Option<Map<String, Value>> {
    let mut body = String::new();
    std::io::stdin()
        .take(MAX_HELPER_STDIN_BYTES)
        .read_to_string(&mut body)
        .ok()?;
    match serde_json::from_str::<Value>(&body) {
        Ok(Value::Object(map)) => Some(map),
        _ => None,
    }
}

/// Copy only the named string fields. A field that is present but not a string
/// is dropped rather than coerced.
fn pick(body: &Map<String, Value>, fields: &[&str]) -> Map<String, Value> {
    let mut out = Map::new();
    for field in fields {
        if let Some(value) = body.get(*field).filter(|v| v.is_string()) {
            out.insert((*field).to_string(), value.clone());
        }
    }
    out
}

#[cfg(test)]
#[path = "helper_tests.rs"]
mod tests;
