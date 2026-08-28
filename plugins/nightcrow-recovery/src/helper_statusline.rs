//! The one line Claude Code renders, and who gets to write it.
//!
//! Installing this plugin necessarily takes the user's statusline away:
//! `statusLine` in `settings.json` holds one command, not a list, so ours
//! replaces whatever was there. Chaining is the only way to give it back —
//! install recorded the value it displaced in a sidecar, and every refresh
//! runs that command with the very bytes Claude Code sent us and prints what
//! it printed. This plugin's own two-number line ([`render_statusline`])
//! stands in only when there is nothing to chain to. Running the command
//! itself is [`delegate`]'s job.
//!
//! Nothing here fails upwards. A statusline that shows an error is worse than
//! a plain one, so every disappointment ends in the same place: our own line,
//! printed as if no chaining had been attempted.

use crate::hooks::{SettingsPaths, displaced_statusline, is_ours};
use serde_json::{Map, Value};
use std::time::Duration;

#[path = "helper_delegate.rs"]
mod delegate;

/// Shown when the statusline payload carries no usage numbers — which is normal:
/// `rate_limits` is absent for accounts without a subscription window and before
/// the session's first response.
const STATUSLINE_FALLBACK: &str = "nightcrow: watching";

/// How long a displaced statusline command may take before we give up on it.
///
/// Claude Code documents no timeout for a statusline and cancels an in-flight
/// script when the next update arrives, so the provider is already the one
/// deciding we took too long. This bound is for the other direction: a command
/// that never returns must not make this process immortal. Two seconds is
/// generous even for the `git`-shelling scripts the provider's own guidance
/// calls slow, and inside the five seconds this plugin asks Claude Code to
/// allow its hook — the most patience anything here claims of the provider.
pub(super) const BUDGET: Duration = Duration::from_secs(2);

const TYPE_KEY: &str = "type";
const COMMAND_KEY: &str = "command";
/// The only `type` of `statusLine` entry there is anything for us to run.
const COMMAND_TYPE: &str = "command";

/// The `statusLine` install displaced, when there is one to chain to. Every way
/// there can be nothing — no `HOME` to look under, no sidecar because we displaced
/// nothing, a sidecar we cannot read — reads the same from here.
pub(super) fn displaced() -> Option<Value> {
    displaced_statusline(&SettingsPaths::discover().ok()?)
}

/// The line to print for this refresh: the displaced command's, when there is one
/// that can produce it, and ours otherwise.
pub(super) fn line(
    displaced: Option<&Value>,
    raw: &[u8],
    rate_limits: Option<&Map<String, Value>>,
    budget: Duration,
) -> String {
    delegated(displaced, raw, budget).unwrap_or_else(|| render_statusline(rate_limits))
}

fn delegated(displaced: Option<&Value>, raw: &[u8], budget: Duration) -> Option<String> {
    let command = command_of(displaced?)?;
    // Our own command in the sidecar would be a chain that runs this binary from
    // itself, and again from there. `is_ours` is the same substring test install and
    // uninstall recognise our entries by, so what is refused here is exactly what
    // those two already consider ours.
    if is_ours(command) {
        return None;
    }
    delegate::capture(command, raw, budget)
}

/// The command string inside whatever Claude Code allowed as a `statusLine`.
///
/// Install recorded that value verbatim, so this reads the shape the provider
/// documents and the one this plugin itself writes — an object with `type` and
/// `command` — and also accepts a bare string, which is the obvious
/// hand-written form. A value with some other `type` is a statusline we do not
/// know how to run, and guessing at it is worse than standing in for it.
///
/// The entry's other fields are Claude Code's to act on, not ours.
fn command_of(value: &Value) -> Option<&str> {
    let command = match value {
        Value::String(command) => command.as_str(),
        Value::Object(map) => {
            let declared = map.get(TYPE_KEY).and_then(Value::as_str);
            if declared.is_some_and(|kind| kind != COMMAND_TYPE) {
                return None;
            }
            map.get(COMMAND_KEY)?.as_str()?
        }
        _ => return None,
    };
    let command = command.trim();
    (!command.is_empty()).then_some(command)
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

#[cfg(test)]
#[path = "helper_statusline_tests.rs"]
mod tests;
