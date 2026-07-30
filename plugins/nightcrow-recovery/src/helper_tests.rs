//! The whitelisting and the statusline text. Reading stdin and connecting to the
//! socket are covered by `ipc_tests`; what matters here is that nothing outside
//! the whitelist can be forwarded, whatever a provider puts in its payload.

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
