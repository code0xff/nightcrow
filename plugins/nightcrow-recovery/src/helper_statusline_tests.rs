//! Who writes the line, and what happens when the displaced command will not.
//!
//! These tests really do spawn shells, which is the point of them: how a
//! statusline command behaves towards us — printing, failing, hanging, shouting on
//! stderr — is only worth pinning if a real one gets to do it.

use super::*;
use std::time::Instant;

/// A budget no test in here is meant to reach. Only the wedged case waits.
const ENOUGH: Duration = Duration::from_secs(5);

/// The budget for a command that will never finish. Short enough not to be felt,
/// long enough that expiry is a decision rather than a lost race with `sh`.
const BRIEF: Duration = Duration::from_millis(150);

/// What `render_statusline` makes of [`limits`], so `OURS` in an assertion reads
/// as "the chain was declined or came to nothing".
const OURS: &str = "5h 40%";

/// The shape this plugin writes into `statusLine`, and so the shape it records
/// when it displaces one.
fn entry(command: &str) -> Value {
    serde_json::json!({ "type": "command", "command": command, "padding": 2 })
}

fn limits() -> Map<String, Value> {
    match serde_json::json!({"five_hour": {"used_percentage": 40.0}}) {
        Value::Object(map) => map,
        _ => unreachable!("the fixture is an object"),
    }
}

/// What a refresh prints when install recorded `displaced`, with usage numbers
/// always there to fall back on.
fn rendered(displaced: &Value, raw: &[u8], budget: Duration) -> String {
    line(Some(displaced), raw, Some(&limits()), budget)
}

#[test]
fn the_displaced_commands_own_line_becomes_our_line() {
    let displaced = entry("echo 'hud | main | 12%'");

    let printed = rendered(&displaced, b"{}", ENOUGH);

    assert_eq!(printed, "hud | main | 12%");
}

#[test]
fn the_bytes_claude_code_sent_reach_the_displaced_command_unchanged() {
    // Key order, number formatting and string escapes are all the provider's, and
    // a parsed and re-encoded copy would keep none of the three.
    let raw = br#"{"zeta":1,"alpha":2.50,"big":1e3,"who":"a\/b"}"#;

    let printed = rendered(&entry("cat"), raw, ENOUGH);

    assert_eq!(printed.as_bytes(), raw);
}

#[test]
fn a_displaced_statusline_recorded_as_a_bare_string_is_run_too() {
    let displaced = Value::String("echo theirs".to_string());

    assert_eq!(rendered(&displaced, b"{}", ENOUGH), "theirs");
}

#[test]
fn a_multi_line_displaced_statusline_keeps_its_own_line_breaks() {
    let displaced = entry("printf 'top\\nbottom\\n'");

    assert_eq!(rendered(&displaced, b"{}", ENOUGH), "top\nbottom");
}

#[test]
fn with_nothing_displaced_our_own_line_is_printed() {
    let numbers = limits();

    assert_eq!(line(None, b"{}", Some(&numbers), ENOUGH), OURS);
    // A JSON null is what install records when it displaced no statusline at all.
    assert_eq!(rendered(&Value::Null, b"{}", ENOUGH), OURS);
    assert_eq!(line(None, b"{}", None, ENOUGH), STATUSLINE_FALLBACK);
}

#[test]
fn a_displaced_command_that_fails_falls_back_however_much_it_printed() {
    let displaced = entry("echo half-a-line; exit 3");

    assert_eq!(rendered(&displaced, b"{}", ENOUGH), OURS);
}

#[test]
fn a_displaced_command_that_cannot_be_run_falls_back() {
    let displaced = entry("/nonexistent/statusline-c0ffee --now");

    assert_eq!(rendered(&displaced, b"{}", ENOUGH), OURS);
}

#[test]
fn a_displaced_command_that_prints_nothing_usable_falls_back() {
    for silent in ["true", "printf '\\n\\n'", "printf '   '"] {
        let printed = rendered(&entry(silent), b"{}", ENOUGH);

        assert_eq!(printed, OURS, "{silent}");
    }
}

#[test]
fn a_displaced_commands_stderr_never_reaches_the_statusline() {
    let displaced = entry("echo noise >&2; echo theirs");

    assert_eq!(rendered(&displaced, b"{}", ENOUGH), "theirs");
}

#[test]
fn a_displaced_command_that_never_finishes_is_given_up_on_and_falls_back() {
    let displaced = entry("sleep 30");
    let started = Instant::now();

    let printed = rendered(&displaced, b"{}", BRIEF);

    assert_eq!(printed, OURS);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the refresh waited {:?} on a command it had given up on",
        started.elapsed()
    );
}

#[test]
fn our_own_command_is_never_run_from_our_own_statusline() {
    // Both of these would print if they ran, and both carry the marker install
    // and uninstall recognise us by, so neither may: a sidecar naming this binary
    // would otherwise chain the statusline into itself.
    for ours in [
        "echo nightcrow-recovery statusline",
        "/opt/nightcrow/libexec/nightcrow-recovery statusline || echo theirs",
    ] {
        let printed = rendered(&entry(ours), b"{}", ENOUGH);

        assert_eq!(printed, OURS, "{ours}");
    }
}

#[test]
fn a_displaced_value_we_cannot_execute_falls_back_rather_than_guessing() {
    // The first is the case that matters: a future `type` may mean something
    // entirely unlike a shell command, so its `command` is not ours to run.
    for unrunnable in [
        serde_json::json!({"type": "some-future-kind", "command": "echo theirs"}),
        serde_json::json!({"type": "command", "command": 42}),
        serde_json::json!({"type": "command"}),
        serde_json::json!({"command": "   "}),
        serde_json::json!([{"command": "echo theirs"}]),
        serde_json::json!(7),
    ] {
        let printed = rendered(&unrunnable, b"{}", ENOUGH);

        assert_eq!(printed, OURS, "{unrunnable}");
    }
}

#[test]
fn a_statusline_reports_the_usage_of_every_window_the_provider_gave() {
    let limits = match serde_json::json!({
        "five_hour": {"used_percentage": 23.5, "resets_at": 1_767_225_600i64},
        "seven_day": {"used_percentage": 41.2, "resets_at": 1_767_657_600i64}
    }) {
        Value::Object(map) => map,
        _ => unreachable!("the fixture is an object"),
    };
    assert_eq!(render_statusline(Some(&limits)), "5h 24% | 7d 41%");
}

#[test]
fn a_statusline_with_one_window_reports_only_that_window() {
    let limits = match serde_json::json!({"seven_day": {"used_percentage": 8.0}}) {
        Value::Object(map) => map,
        _ => unreachable!("the fixture is an object"),
    };
    assert_eq!(render_statusline(Some(&limits)), "7d 8%");
}

#[test]
fn a_statusline_still_prints_a_line_when_the_provider_reported_no_windows() {
    assert_eq!(render_statusline(None), STATUSLINE_FALLBACK);
    assert_eq!(render_statusline(Some(&Map::new())), STATUSLINE_FALLBACK);
    let unusable = match serde_json::json!({"five_hour": {"used_percentage": "lots"}}) {
        Value::Object(map) => map,
        _ => unreachable!("the fixture is an object"),
    };
    assert_eq!(render_statusline(Some(&unusable)), STATUSLINE_FALLBACK);
}
