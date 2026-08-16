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
//!
//! The one thing that leaves this process whole is the statusline payload handed
//! to the command we displaced (see [`status_line`]) — and that command was being
//! given the same bytes by Claude Code before this plugin was installed. We
//! narrow what we keep; we do not narrow what someone else was already told.

use crate::ipc::{IpcMessage, send, socket_path};
use crate::protocol::PANE_TOKEN_ENV;
use crate::provider::SignalKind;
use serde_json::{Map, Value};
use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

#[path = "helper_statusline.rs"]
mod status_line;

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

/// Forward a `StopFailure` payload. Always succeeds from the caller's point of
/// view; `StopFailure` ignores our exit code anyway, and a hook that fails
/// loudly would be worse than one that does nothing.
pub fn hook() -> ExitCode {
    if let Some((token, payload)) = parse_object(&read_stdin_bytes()).and_then(|body| {
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

/// Report that a turn ended, so the host can raise the pane's attention marker.
///
/// Sends no payload: `Stop` fires whatever the outcome, and which outcome it was
/// is not something the marker distinguishes. Reading stdin is still necessary —
/// Claude Code writes the hook payload there and a helper that never drained it
/// would leave the provider writing into a full pipe.
pub fn turn_end() -> ExitCode {
    let _ = read_stdin_bytes();
    if let Some(token) = pane_token() {
        let _ = send(
            &socket_path().unwrap_or_default(),
            &IpcMessage {
                token,
                kind: SignalKind::TurnEnd,
                payload: Value::Null,
            },
        );
    }
    ExitCode::SUCCESS
}

/// Forward the statusline's `rate_limits` and print a line — the line the user's
/// own statusline command printed, whenever installing this plugin displaced one.
/// Claude Code's `statusLine` holds a single command, so the only way not to cost
/// the user their statusline is to run it from ours; see [`status_line`].
pub fn statusline() -> ExitCode {
    let raw = read_stdin_bytes();
    let displaced = status_line::displaced();
    let refresh = refresh(&raw, displaced.as_ref(), status_line::BUDGET);
    if let (Some(token), Some(limits)) = (pane_token(), refresh.rate_limits) {
        let _ = send(
            &socket_path().unwrap_or_default(),
            &IpcMessage {
                token,
                kind: SignalKind::RateLimits,
                payload: Value::Object(limits),
            },
        );
    }
    println!("{}", refresh.line);
    ExitCode::SUCCESS
}

/// What one statusline refresh comes to: the usage numbers to forward, and the
/// line to print.
struct Refresh {
    rate_limits: Option<Map<String, Value>>,
    line: String,
}

/// Decide both halves of a refresh without touching the socket or stdout, so
/// every way a displaced command can disappoint us stays testable — and so the
/// usage numbers are read out of the payload before anything is delegated, which
/// is what keeps a misbehaving statusline command from costing us them.
fn refresh(raw: &[u8], displaced: Option<&Value>, budget: Duration) -> Refresh {
    let rate_limits = parse_object(raw).and_then(rate_limits_of);
    let line = status_line::line(displaced, raw, rate_limits.as_ref(), budget);
    Refresh { rate_limits, line }
}

/// The usage windows of a statusline payload, when it reported any.
fn rate_limits_of(body: Map<String, Value>) -> Option<Map<String, Value>> {
    body.get(RATE_LIMITS_FIELD)?.as_object().cloned()
}

/// The pane this helper belongs to, from the environment its provider inherited.
/// Absent means this provider was not started by nightcrow, so there is nothing
/// to correlate and nothing to send.
fn pane_token() -> Option<String> {
    std::env::var(PANE_TOKEN_ENV)
        .ok()
        .filter(|t| !t.trim().is_empty())
}

/// Every byte the provider wrote, kept exactly as it wrote them. The statusline
/// helper hands these on to the command it displaced, and a re-serialised copy is
/// not the same thing: key order and number formatting are the provider's to
/// choose, and a command that was reading its input before we existed should not
/// find it rearranged now. A read that fails part-way keeps what did arrive —
/// unparseable, and treated as such below.
fn read_stdin_bytes() -> Vec<u8> {
    let mut raw = Vec::new();
    let _ = std::io::stdin()
        .take(MAX_HELPER_STDIN_BYTES)
        .read_to_end(&mut raw);
    raw
}

/// The payload as an object, when that is what it is. Anything else is not an
/// error here: there is simply nothing of ours to forward out of it.
fn parse_object(raw: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice::<Value>(raw) {
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
