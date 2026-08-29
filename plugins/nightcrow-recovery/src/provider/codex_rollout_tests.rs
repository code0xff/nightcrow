use super::*;
use serde_json::json;

/// A plausible "now" (2027-01-15) for deadline checks: well past the minimum
/// plausible epoch, so only the value under test decides the outcome.
const NOW: i64 = 1_800_000_000;
/// Inside the accepted horizon.
const SOON: i64 = NOW + 3_600;
/// Outside it (30 days ahead, against an 8-day horizon).
const TOO_FAR: i64 = NOW + 30 * 24 * 60 * 60;

const UUID: &str = "0199cbb1-2b70-7f11-9f0f-0f8e9d1c2b3a";

fn record(tag: &str, payload: Value) -> String {
    json!({"timestamp":"2027-01-15T00:00:00Z","ordinal":7,"type":tag,"payload":payload}).to_string()
}

fn deadline_for(payload: Value) -> Option<i64> {
    match classify_line(&record("token_count", payload), NOW) {
        Some(Record::TokenCount { resets_at, .. }) => resets_at,
        other => panic!("expected a token_count record, got {other:?}"),
    }
}

fn rate_limits(primary: Value) -> Value {
    json!({"rate_limits": {"primary": primary}})
}

#[test]
fn session_id_from_filename_on_a_valid_rollout_name_returns_the_uuid() {
    let name = format!("rollout-2026-07-30T19-23-45-{UUID}.jsonl");
    assert_eq!(session_id_from_filename(&name).as_deref(), Some(UUID));
}

#[test]
fn session_id_from_filename_without_a_uuid_returns_none() {
    assert_eq!(
        session_id_from_filename("rollout-2026-07-30T19-23-45.jsonl"),
        None
    );
}

#[test]
fn session_id_from_filename_on_an_empty_string_returns_none() {
    assert_eq!(session_id_from_filename(""), None);
}

#[test]
fn session_id_from_filename_with_a_wrong_prefix_returns_none() {
    let name = format!("session-2026-07-30T19-23-45-{UUID}.jsonl");
    assert_eq!(session_id_from_filename(&name), None);
}

#[test]
fn session_id_from_filename_with_a_wrong_extension_returns_none() {
    let name = format!("rollout-2026-07-30T19-23-45-{UUID}.json");
    assert_eq!(session_id_from_filename(&name), None);
}

#[test]
fn session_id_from_filename_with_a_non_hex_uuid_group_returns_none() {
    let name = "rollout-2026-07-30T19-23-45-zzzzzzzz-2b70-7f11-9f0f-0f8e9d1c2b3a.jsonl";
    assert_eq!(session_id_from_filename(name), None);
}

#[test]
fn a_session_meta_prefers_the_payload_id() {
    let line = record("session_meta", json!({"id": UUID, "session_id": "other"}));
    assert_eq!(
        classify_line(&line, NOW),
        Some(Record::SessionMeta {
            id: Some(UUID.to_string())
        })
    );
}

#[test]
fn a_session_meta_falls_through_to_conversation_id() {
    let line = record("session_meta", json!({"conversation_id": UUID}));
    assert_eq!(
        classify_line(&line, NOW),
        Some(Record::SessionMeta {
            id: Some(UUID.to_string())
        })
    );
}

#[test]
fn a_session_meta_whose_id_is_not_argv_safe_reports_no_id() {
    let line = record("session_meta", json!({"id": "id with space; rm -rf /"}));
    assert_eq!(
        classify_line(&line, NOW),
        Some(Record::SessionMeta { id: None })
    );
}

#[test]
fn a_session_meta_without_any_payload_reports_no_id() {
    let line = json!({"type": "session_meta"}).to_string();
    assert_eq!(
        classify_line(&line, NOW),
        Some(Record::SessionMeta { id: None })
    );
}

#[test]
fn a_token_count_with_a_valid_resets_at_is_accepted() {
    assert_eq!(
        deadline_for(rate_limits(json!({"resets_at": SOON}))),
        Some(SOON)
    );
}

#[test]
fn a_resets_at_that_is_missing_is_rejected() {
    assert_eq!(deadline_for(rate_limits(json!({"used_percent": 90}))), None);
}

#[test]
fn a_resets_at_that_is_null_is_rejected() {
    assert_eq!(deadline_for(rate_limits(json!({"resets_at": null}))), None);
}

#[test]
fn a_resets_at_that_is_a_string_is_rejected() {
    let payload = rate_limits(json!({"resets_at": SOON.to_string()}));
    assert_eq!(deadline_for(payload), None);
}

#[test]
fn a_resets_at_that_is_negative_is_rejected() {
    assert_eq!(deadline_for(rate_limits(json!({"resets_at": -1}))), None);
}

#[test]
fn a_resets_at_far_in_the_future_is_rejected() {
    assert_eq!(
        deadline_for(rate_limits(json!({"resets_at": TOO_FAR}))),
        None
    );
}

#[test]
fn a_token_count_without_rate_limits_is_rejected() {
    assert_eq!(deadline_for(json!({"total_tokens": 12})), None);
}

#[test]
fn a_token_count_without_primary_is_rejected() {
    let payload = json!({"rate_limits": {"secondary": {"resets_at": SOON}}});
    assert_eq!(deadline_for(payload), None);
}

#[test]
fn a_turn_complete_with_usage_limit_exceeded_is_a_limit() {
    let payload = json!({"error": {"codex_error_info": USAGE_LIMIT_ERROR_INFO}});
    assert_eq!(
        classify_line(&record("turn_complete", payload), NOW),
        Some(Record::UsageLimit)
    );
}

#[test]
fn a_turn_complete_with_another_error_is_not_classified() {
    let payload = json!({"error": {"codex_error_info": "stream_disconnected"}});
    assert_eq!(classify_line(&record("turn_complete", payload), NOW), None);
}

#[test]
fn a_turn_complete_without_an_error_is_not_classified() {
    let payload = json!({"usage": {"input_tokens": 10}});
    assert_eq!(classify_line(&record("turn_complete", payload), NOW), None);
}

#[test]
fn a_malformed_line_is_not_classified() {
    assert_eq!(classify_line("{\"type\":\"token_count\"", NOW), None);
    assert_eq!(classify_line("", NOW), None);
}

#[test]
fn a_record_without_a_string_type_is_not_classified() {
    assert_eq!(classify_line(&json!({"type": 3}).to_string(), NOW), None);
}

#[test]
fn an_unknown_record_type_is_not_classified() {
    assert_eq!(classify_line(&record("event_msg", json!({})), NOW), None);
}

#[test]
fn an_over_long_line_is_not_classified() {
    let pad = "a".repeat(MAX_RECORD_BYTES);
    let line = record("token_count", json!({"pad": pad}));
    assert!(line.len() > MAX_RECORD_BYTES);
    assert_eq!(classify_line(&line, NOW), None);
}

#[test]
fn an_empty_or_over_long_session_id_is_not_argv_safe() {
    assert!(!valid_session_id(""));
    assert!(!valid_session_id(&"a".repeat(MAX_SESSION_ID_BYTES + 1)));
    assert!(valid_session_id(UUID));
    assert!(valid_session_id("plain_id-1"));
}
