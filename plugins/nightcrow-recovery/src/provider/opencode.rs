//! OpenCode adapter — deliberately observe-only.
//!
//! OpenCode retries a retryable API error *without bound*: there is no
//! max-attempt constant, the backoff starts at 2 s and doubles, and the 30 s cap
//! applies only when the response carried no `retry-after` header — with one the
//! cap is ~24.8 days. So "wait for the retries to run out" is a state this
//! adapter can never reach, and a pane in `retry` is hands off: no input, no
//! relaunch, no abort. It only reports, and only once the retry is demonstrably
//! over — the session went `idle`, or the process exited.
//!
//! State comes from the local server's `GET /session/status`, which is
//! first-class server state rather than screen scraping. Terminal text is not
//! consulted at all; see [`Provider::on_output`] below for why.

use super::{LimitEvent, PaneContext, Provider, ResumePlan};
use crate::protocol::PaneGeneration;
use std::time::Duration;

#[path = "opencode_http.rs"]
mod http;

pub use http::{
    SessionStatus, StatusKind, StatusSource, http_get, interpret_next, parse_status_body,
};

/// Port OpenCode's local server binds unless `--port` says otherwise.
pub const DEFAULT_PORT: u16 = 4096;

/// Override for a user who always runs the server elsewhere. A `--port` on the
/// pane's own command line wins over it: that is the truth about *this* process.
const PORT_ENV: &str = "NIGHTCROW_OPENCODE_PORT";

/// Snapshot of every session the server knows about.
const STATUS_PATH: &str = "/session/status";

/// Flags that carry the server port on a command line.
const PORT_FLAGS: &[&str] = &["--port", "-p"];

/// Shortest gap between two status requests. The host's timer can tick far
/// faster than this, and a snapshot is not worth a request per tick.
const MIN_POLL_INTERVAL_SECS: i64 = 5;

/// Socket budget for one status request. Loopback, so anything slower means the
/// server is wedged and waiting longer would only stall the plugin's loop.
const HTTP_TIMEOUT: Duration = Duration::from_millis(1_500);

/// A session id is handed back as a command-line argument, so it is bounded.
/// Loose but finite: real ids are short, and an over-long value is a bug.
const MAX_SESSION_ID_BYTES: usize = 64;

/// Resumes a named session. Never `--continue`/`-c`, which resumes *the last*
/// session and would happily pick up another pane's work.
const RESUME_FLAG: &str = "--session";

const RETRY_ENDED_DETAIL: &str = "opencode stopped retrying without producing a result";
const ALIVE_HOLD: &str = "opencode retries internally without bound; never interrupt a retry";
const NO_SESSION_HOLD: &str =
    "no opencode session id; --continue could resume another pane's session";

/// A retry this adapter saw, and what it managed to learn from it.
#[derive(Debug, Clone, Default)]
struct Retrying {
    session_id: Option<String>,
    resets_at: Option<i64>,
}

/// Adapter state for one pane. Nothing here is written to disk, and no message
/// text from a status payload is retained.
#[derive(Debug, Default)]
pub struct OpenCode {
    /// `None` until something states a port, so the env fallback stays lazy and
    /// an explicit `--port` is distinguishable from the default.
    port: Option<u16>,
    /// Injected snapshot source. `None` means the HTTP source, resolved per poll
    /// so [`Self::observe_command`] can still change the port.
    source: Option<Box<dyn StatusSource>>,
    /// Generation this state belongs to; a change re-arms everything.
    generation: Option<PaneGeneration>,
    retrying: Option<Retrying>,
    exited: bool,
    fired: bool,
    last_poll: Option<i64>,
}

impl OpenCode {
    /// Testing seam: replace the status source. The HTTP source is the default.
    #[cfg(test)]
    pub fn with_status_source(source: Box<dyn StatusSource>) -> Self {
        Self {
            source: Some(source),
            ..Self::default()
        }
    }

    /// Port this adapter would query.
    pub fn port(&self) -> u16 {
        self.port.or_else(env_port).unwrap_or(DEFAULT_PORT)
    }

    /// Learn the port from the pane's own command line (`--port N` / `-p N`). An
    /// absent flag or an unparsable value leaves the port as it was.
    pub fn observe_command(&mut self, command: &str) {
        if let Some(port) = port_from_command(command) {
            self.port = Some(port);
        }
    }

    fn sync_generation(&mut self, ctx: &PaneContext) {
        if let Some(command) = &ctx.command {
            self.observe_command(command);
        }
        if self.generation == Some(ctx.generation) {
            return;
        }
        self.generation = Some(ctx.generation);
        // A respawn is a different process and a different session: nothing
        // learned about the old one may be reported against the new one.
        self.retrying = None;
        self.exited = false;
        self.fired = false;
        self.last_poll = None;
    }

    fn fetch_status(&mut self) -> anyhow::Result<String> {
        match &mut self.source {
            Some(source) => source.fetch(),
            None => http_get(self.port(), STATUS_PATH, HTTP_TIMEOUT),
        }
    }

    fn due(&self, now_epoch: i64) -> bool {
        match self.last_poll {
            None => true,
            // A clock that moved backwards also falls outside the window, which
            // re-arms polling instead of stalling until the clock catches up.
            Some(last) => !(last..last.saturating_add(MIN_POLL_INTERVAL_SECS)).contains(&now_epoch),
        }
    }

    fn remember_retry(&mut self, status: &SessionStatus, now_epoch: i64) {
        let resets_at = match status.kind {
            StatusKind::Retry {
                next: Some(next), ..
            } => interpret_next(next, now_epoch),
            _ => None,
        };
        let known = self.retrying.get_or_insert_default();
        // A later snapshot may omit what an earlier one told us, so only ever
        // fill a gap — never overwrite a known value with None.
        if status.session_id.is_some() {
            known.session_id = status.session_id.clone();
        }
        if resets_at.is_some() {
            known.resets_at = resets_at;
        }
    }

    /// Did the session we watched retrying go idle in this snapshot?
    fn went_idle(&self, statuses: &[SessionStatus]) -> bool {
        let watched = self.retrying.as_ref().and_then(|r| r.session_id.as_deref());
        statuses.iter().any(|s| {
            matches!(s.kind, StatusKind::Idle)
                && match (watched, s.session_id.as_deref()) {
                    (Some(want), Some(got)) => want == got,
                    // With an id missing on either side the snapshot cannot be
                    // attributed; counting it is the only alternative to never
                    // reporting at all.
                    _ => true,
                }
        })
    }

    fn emit(&mut self, now_epoch: i64) -> Option<LimitEvent> {
        let retrying = self.retrying.clone()?;
        self.fired = true;
        Some(LimitEvent::usage(
            retrying
                .session_id
                .as_deref()
                .and_then(validated_session_id),
            // A deadline already past is worse than none: it would tell the
            // machine to resume immediately, into the same limit.
            retrying.resets_at.filter(|at| *at > now_epoch),
            RETRY_ENDED_DETAIL,
        ))
    }
}

impl Provider for OpenCode {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn on_output(
        &mut self,
        _ctx: &PaneContext,
        _text: &str,
        _now_epoch: i64,
    ) -> Option<LimitEvent> {
        // Intentionally blind: OpenCode's TUI retry format string is unverified,
        // so any needle list here would be a guess, and a wrong guess parks a
        // healthy pane. The status endpoint is authoritative; the screen is not.
        None
    }

    fn poll(&mut self, ctx: &PaneContext, now_epoch: i64) -> Option<LimitEvent> {
        self.sync_generation(ctx);
        if self.fired {
            return None;
        }
        // The process is gone, so the last thing we saw is final and there is
        // nothing left on the server worth asking about.
        if self.exited {
            return self.emit(now_epoch);
        }
        if !self.due(now_epoch) {
            return None;
        }
        self.last_poll = Some(now_epoch);
        // No server, a non-200, or an unreadable body is ordinary — the user need
        // not be running the server at all. Swallow it, and let the interval keep
        // the next attempt from becoming a tight loop.
        let statuses = parse_status_body(&self.fetch_status().ok()?);
        if let Some(status) = statuses
            .iter()
            .find(|s| matches!(s.kind, StatusKind::Retry { .. }))
        {
            self.remember_retry(status, now_epoch);
            return None;
        }
        if self.retrying.is_some() && self.went_idle(&statuses) {
            return self.emit(now_epoch);
        }
        None
    }

    fn on_exit(&mut self, ctx: &PaneContext) {
        self.sync_generation(ctx);
        self.exited = true;
    }

    fn resume(&self, _ctx: &PaneContext, limit: &LimitEvent, alive: bool) -> Option<ResumePlan> {
        // Never `ResumePlan::Input`: a live pane may be mid-retry, and typing at
        // one is exactly what this adapter exists to avoid.
        if alive {
            return Some(ResumePlan::Hold(ALIVE_HOLD));
        }
        let Some(id) = limit.session_id.as_deref().and_then(validated_session_id) else {
            return Some(ResumePlan::Hold(NO_SESSION_HOLD));
        };
        Some(ResumePlan::Relaunch(vec![RESUME_FLAG.to_string(), id]))
    }
}

/// First `--port`/`-p` value on a command line, in either `--port N` or
/// `--port=N` form. `None` for an absent flag, a non-numeric value, or port 0,
/// which nothing can be reached at.
fn port_from_command(command: &str) -> Option<u16> {
    let mut tokens = command.split_whitespace();
    while let Some(token) = tokens.next() {
        let value = match token.split_once('=') {
            Some((flag, inline)) if PORT_FLAGS.contains(&flag) => inline,
            _ if PORT_FLAGS.contains(&token) => tokens.next()?,
            _ => continue,
        };
        return value.parse().ok().filter(|p| *p != 0);
    }
    None
}

fn env_port() -> Option<u16> {
    std::env::var(PORT_ENV)
        .ok()?
        .parse()
        .ok()
        .filter(|p| *p != 0)
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
#[path = "opencode_tests.rs"]
mod tests;
