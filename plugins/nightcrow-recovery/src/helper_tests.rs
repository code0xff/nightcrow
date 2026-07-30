//! The whitelisting, and what one refresh decides. Reading stdin and connecting
//! to the socket are covered by `ipc_tests`; what matters here is that nothing
//! outside the whitelist can be forwarded, whatever a provider puts in its
//! payload, and that nothing a displaced statusline command does can lose the
//! usage numbers on the way. Which command gets to print is `helper_statusline`'s.

use super::*;

fn stop_failure_payload() -> Map<String, Value> {
    match serde_json::json!({
        "session_id": "11111111-2222-3333-4444-555555555555",
        "prompt_id": "p_1",
        "transcript_path": "/home/x/.claude/projects/repo/session.jsonl",
        "cwd": "/w/repo",
        "hook_event_name": "StopFailure",
        "error_type": "rate_limit",
        "error_message": "You have exceeded your usage for account billing@example.com",
        "agent_id": "a_1"
    }) {
        Value::Object(map) => map,
        _ => unreachable!("the fixture is an object"),
    }
}

#[test]
fn only_the_three_whitelisted_hook_fields_are_forwarded() {
    let picked = pick(&stop_failure_payload(), &STOP_FAILURE_FIELDS);
    let mut keys: Vec<&String> = picked.keys().collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["error_type", "hook_event_name", "session_id"],
        "nothing else may leave the provider's process"
    );
}

#[test]
fn a_transcript_path_and_an_error_message_are_never_forwarded() {
    let picked = pick(&stop_failure_payload(), &STOP_FAILURE_FIELDS);
    assert!(picked.get("transcript_path").is_none());
    assert!(picked.get("error_message").is_none());
    assert!(picked.get("cwd").is_none());
    assert!(picked.get("prompt_id").is_none());
    let serialised = Value::Object(picked).to_string();
    assert!(!serialised.contains("billing@example.com"), "{serialised}");
}

#[test]
fn a_whitelisted_field_of_the_wrong_type_is_dropped_rather_than_coerced() {
    let payload = match serde_json::json!({
        "session_id": 42,
        "error_type": null,
        "hook_event_name": {"name": "StopFailure"}
    }) {
        Value::Object(map) => map,
        _ => unreachable!("the fixture is an object"),
    };
    assert!(pick(&payload, &STOP_FAILURE_FIELDS).is_empty());
}

#[test]
fn a_payload_missing_every_whitelisted_field_forwards_nothing() {
    assert!(pick(&Map::new(), &STOP_FAILURE_FIELDS).is_empty());
}

/// A statusline payload with usage numbers in it, as the bytes a provider would
/// have written them.
const STATUSLINE_BODY: &[u8] =
    br#"{"session_id":"s","rate_limits":{"five_hour":{"used_percentage":40.0}}}"#;

/// Only the command that hangs spends this, and no assertion below turns on
/// whether the others managed to print in time.
const BUDGET: Duration = Duration::from_millis(200);

/// A generous budget, for the cases that are about what a command printed.
const ENOUGH: Duration = Duration::from_secs(5);

fn statusline_entry(command: &str) -> Value {
    serde_json::json!({ "type": "command", "command": command })
}

fn five_hour(refresh: &Refresh) -> Option<&Value> {
    refresh.rate_limits.as_ref()?.get("five_hour")
}

#[test]
fn a_refresh_forwards_the_usage_numbers_whatever_the_displaced_command_does() {
    let expected = serde_json::json!({"used_percentage": 40.0});
    for displaced in [
        Value::Null,
        statusline_entry("echo theirs"),
        statusline_entry("exit 1"),
        statusline_entry("sleep 30"),
        statusline_entry("/x/nightcrow-recovery statusline"),
        serde_json::json!({"type": "some-future-kind"}),
    ] {
        let refresh = refresh(STATUSLINE_BODY, Some(&displaced), BUDGET);

        let forwarded = five_hour(&refresh);

        assert_eq!(forwarded, Some(&expected), "lost them for {displaced}");
    }
}

#[test]
fn a_refresh_prints_the_displaced_statuslines_line_rather_than_our_own() {
    let displaced = statusline_entry("echo theirs");

    let refresh = refresh(STATUSLINE_BODY, Some(&displaced), ENOUGH);

    assert_eq!(refresh.line, "theirs");
}

#[test]
fn a_refresh_with_nothing_displaced_prints_the_numbers_it_forwarded() {
    let refresh = refresh(STATUSLINE_BODY, None, BUDGET);

    assert_eq!(refresh.line, "5h 40%");
}

#[test]
fn a_body_we_cannot_parse_still_reaches_the_displaced_command() {
    // Parsing is for the fields we forward; the bytes are the displaced command's
    // business, and a payload we cannot read may well be one it can.
    let displaced = statusline_entry("cat");

    let refresh = refresh(b"not json at all", Some(&displaced), ENOUGH);

    assert!(refresh.rate_limits.is_none());
    assert_eq!(refresh.line, "not json at all");
}
