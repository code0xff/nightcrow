use super::*;

/// Fixed "now" well inside the plausible epoch band, so every case below is
/// decided by the value under test rather than by the clock.
const NOW: i64 = 1_800_000_000;

/// Port used by the path-rejection tests. They must fail before any connect is
/// attempted, so nothing is expected to be listening here.
const UNUSED_PORT: u16 = 4096;

fn refusal(path: &str) -> String {
    http_get(UNUSED_PORT, path, Duration::from_millis(1))
        .unwrap_err()
        .to_string()
}

#[test]
fn an_absolute_epoch_in_seconds_is_taken_as_the_reset_time() {
    assert_eq!(interpret_next(NOW + 3600, NOW), Some(NOW + 3600));
}

#[test]
fn an_absolute_epoch_in_milliseconds_is_divided_down_to_seconds() {
    assert_eq!(interpret_next((NOW + 3600) * 1_000, NOW), Some(NOW + 3600));
}

#[test]
fn a_small_millisecond_delay_is_taken_as_relative_to_now() {
    assert_eq!(interpret_next(30_000, NOW), Some(NOW + 30));
}

#[test]
fn a_sub_second_relative_delay_resolves_to_now() {
    assert_eq!(interpret_next(500, NOW), Some(NOW));
}

#[test]
fn a_next_of_zero_is_not_a_deadline() {
    assert_eq!(interpret_next(0, NOW), None);
}

#[test]
fn a_negative_next_is_not_a_deadline() {
    assert_eq!(interpret_next(-5_000, NOW), None);
    assert_eq!(interpret_next(i64::MIN, NOW), None);
}

#[test]
fn an_absurdly_large_next_is_not_a_deadline() {
    assert_eq!(interpret_next(i64::MAX, NOW), None);
}

#[test]
fn an_absolute_next_just_past_the_reset_horizon_is_not_a_deadline() {
    let past = NOW + MAX_RESET_HORIZON_SECS + 1;
    assert_eq!(interpret_next(past, NOW), None);
    assert_eq!(
        interpret_next(NOW + MAX_RESET_HORIZON_SECS, NOW),
        Some(past - 1)
    );
}

#[test]
fn a_relative_delay_is_accepted_up_to_the_horizon_and_refused_past_it() {
    assert_eq!(
        interpret_next(MAX_RELATIVE_NEXT_MILLIS, NOW),
        Some(NOW + MAX_RESET_HORIZON_SECS)
    );
    assert_eq!(interpret_next(MAX_RELATIVE_NEXT_MILLIS + 1, NOW), None);
}

#[test]
fn an_object_keyed_by_session_id_yields_one_status_per_key() {
    let statuses = parse_status_body(r#"{"ses_abc":{"type":"retry","attempt":3,"next":4000}}"#);
    assert_eq!(
        statuses,
        vec![SessionStatus {
            session_id: Some("ses_abc".to_string()),
            kind: StatusKind::Retry {
                attempt: 3,
                next: Some(4000)
            },
        }]
    );
}

#[test]
fn an_array_of_entries_yields_one_status_per_element() {
    let statuses = parse_status_body(
        r#"[{"sessionID":"a","status":{"type":"busy"}},{"id":"b","type":"idle"}]"#,
    );
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses[0].session_id.as_deref(), Some("a"));
    assert_eq!(statuses[0].kind, StatusKind::Busy);
    assert_eq!(statuses[1].session_id.as_deref(), Some("b"));
    assert_eq!(statuses[1].kind, StatusKind::Idle);
}

#[test]
fn a_retry_without_next_parses_with_no_deadline() {
    let statuses = parse_status_body(r#"{"s":{"type":"retry","attempt":1}}"#);
    assert_eq!(
        statuses[0].kind,
        StatusKind::Retry {
            attempt: 1,
            next: None
        }
    );
}

#[test]
fn a_retry_with_an_unusable_attempt_number_parses_as_attempt_zero() {
    for body in [
        r#"{"s":{"type":"retry","attempt":-1}}"#,
        r#"{"s":{"type":"retry","attempt":"many"}}"#,
        r#"{"s":{"type":"retry"}}"#,
    ] {
        assert_eq!(
            parse_status_body(body)[0].kind,
            StatusKind::Retry {
                attempt: 0,
                next: None
            },
            "body {body}"
        );
    }
}

#[test]
fn a_busy_status_parses_as_busy() {
    assert_eq!(
        parse_status_body(r#"{"s":{"type":"busy"}}"#)[0].kind,
        StatusKind::Busy
    );
}

#[test]
fn an_idle_status_parses_as_idle() {
    assert_eq!(
        parse_status_body(r#"{"s":{"type":"idle"}}"#)[0].kind,
        StatusKind::Idle
    );
}

#[test]
fn an_unrecognised_type_parses_as_unknown() {
    for body in [
        r#"{"s":{"type":"queued"}}"#,
        r#"{"s":{"type":42}}"#,
        r#"{"s":{"type":null}}"#,
    ] {
        assert_eq!(
            parse_status_body(body)[0].kind,
            StatusKind::Unknown,
            "{body}"
        );
    }
}

#[test]
fn an_empty_body_yields_no_statuses() {
    assert!(parse_status_body("").is_empty());
    assert!(parse_status_body("   ").is_empty());
}

#[test]
fn malformed_json_yields_no_statuses() {
    assert!(parse_status_body("{not json").is_empty());
    assert!(parse_status_body(r#"{"s":{"type":"idle"}"#).is_empty());
}

#[test]
fn a_json_scalar_instead_of_a_container_yields_no_statuses() {
    for body in ["42", "null", "true", r#""idle""#] {
        assert!(parse_status_body(body).is_empty(), "{body}");
    }
}

#[test]
fn entries_holding_no_status_object_are_ignored() {
    assert!(parse_status_body(r#"{"a":1,"b":{"foo":2},"c":null}"#).is_empty());
    assert!(parse_status_body(r#"[1,"x",{"status":{}}]"#).is_empty());
}

#[test]
fn a_request_path_containing_a_space_is_refused_before_connecting() {
    assert!(refusal("/session status").contains("refusing"));
}

#[test]
fn a_request_path_containing_a_carriage_return_or_newline_is_refused() {
    assert!(refusal("/a\r\nX-Evil: 1").contains("refusing"));
    assert!(refusal("/a\nb").contains("refusing"));
    assert!(refusal("/a\rb").contains("refusing"));
}

#[test]
fn a_path_that_is_not_an_absolute_ascii_word_path_is_refused() {
    for path in [
        "",
        "session/status",
        "/session?x=1",
        "/session/../etc",
        "/sé",
    ] {
        assert!(!is_safe_path(path), "{path}");
    }
}

#[test]
fn the_status_path_shape_passes_the_path_filter() {
    for path in ["/session/status", "/event", "/a-b_c/9"] {
        assert!(is_safe_path(path), "{path}");
    }
}
