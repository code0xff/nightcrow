use super::*;
use serde_json::json;

/// Feb 2025, comfortably inside the plausible band the shared helpers enforce.
pub(super) const NOW: i64 = 1_738_400_000;
const FIVE_HOUR_RESET: i64 = 1_738_425_600;
const SEVEN_DAY_RESET: i64 = 1_738_857_600;
pub(super) const SESSION: &str = "0199f0aa-1111-4222-8333-abcdef123456";

pub(super) fn ctx(generation: PaneGeneration) -> PaneContext {
    PaneContext {
        token: "pane0".to_string(),
        generation,
        cwd: "/repo".to_string(),
        command: Some("claude".to_string()),
    }
}

fn rate_limits(payload: Value) -> OutOfBand {
    OutOfBand {
        kind: SignalKind::RateLimits,
        payload,
    }
}

fn stop_failure(payload: Value) -> OutOfBand {
    OutOfBand {
        kind: SignalKind::StopFailure,
        payload,
    }
}

/// A realistic hook payload, `error_message` included so tests can prove it never
/// reaches a `detail`.
fn stop_failure_of(error_type: &str) -> OutOfBand {
    stop_failure(json!({
        "hook_event_name": "StopFailure",
        "session_id": SESSION,
        "transcript_path": "/home/u/.claude/t.jsonl",
        "cwd": "/repo",
        "error_type": error_type,
        "error_message": "Quota exceeded for account acct_secret_42",
    }))
}

/// Feed one `rate_limits` object and report the deadline it left behind, which a
/// later StopFailure would carry.
fn deadline_after(payload: Value) -> Option<i64> {
    let mut claude = Claude::default();
    assert_eq!(claude.on_signal(&ctx(1), &rate_limits(payload), NOW), None);
    claude
        .on_signal(&ctx(1), &stop_failure_of("rate_limit"), NOW)
        .expect("rate_limit is a limit")
        .resets_at
}

fn window(resets_at: Value) -> Value {
    json!({"five_hour": {"used_percentage": 99.0, "resets_at": resets_at}})
}

#[test]
fn the_adapter_reports_its_stable_name() {
    assert_eq!(Claude::default().name(), "claude");
}

#[test]
fn both_rate_limit_windows_present_picks_the_earliest_reset() {
    let payload = json!({
        "five_hour": {"used_percentage": 23.5, "resets_at": FIVE_HOUR_RESET},
        "seven_day": {"used_percentage": 41.2, "resets_at": SEVEN_DAY_RESET},
    });
    assert_eq!(deadline_after(payload), Some(FIVE_HOUR_RESET));
}

#[test]
fn a_single_rate_limit_window_is_used_as_the_deadline() {
    let payload = json!({"seven_day": {"used_percentage": 41.2, "resets_at": SEVEN_DAY_RESET}});
    assert_eq!(deadline_after(payload), Some(SEVEN_DAY_RESET));
}

/// The object is absent for non-Pro/Max accounts and empty before the session's
/// first response; a window may also arrive without its `resets_at`.
#[test]
fn an_absent_or_incomplete_rate_limits_object_yields_no_deadline() {
    for payload in [json!({}), json!({"five_hour": {"used_percentage": 12.0}})] {
        assert_eq!(deadline_after(payload.clone()), None, "{payload:?}");
    }
}

/// Null, wrong type, non-positive, and beyond the believable horizon must all
/// degrade to "no deadline" rather than to a wait of the wrong length.
#[test]
fn an_unusable_resets_at_yields_no_deadline() {
    let far = NOW + crate::provider::MAX_RESET_HORIZON_SECS + 1;
    let bad = [
        Value::Null,
        json!("1738425600"),
        json!(-1),
        json!(0),
        json!(far),
        json!(1.5),
    ];
    for value in bad {
        assert_eq!(deadline_after(window(value.clone())), None, "{value:?}");
    }
}

#[test]
fn a_rate_limits_signal_alone_is_never_a_limit_even_at_a_full_window() {
    let payload = json!({
        "five_hour": {"used_percentage": 100.0, "resets_at": FIVE_HOUR_RESET},
        "seven_day": {"used_percentage": 100.0, "resets_at": SEVEN_DAY_RESET},
    });
    let mut claude = Claude::default();
    assert_eq!(claude.on_signal(&ctx(1), &rate_limits(payload), NOW), None);
}

#[test]
fn a_stop_failure_with_error_type_rate_limit_reports_a_usage_limit() {
    let mut claude = Claude::default();
    let event = claude
        .on_signal(&ctx(1), &stop_failure_of("rate_limit"), NOW)
        .expect("rate_limit is a usage limit");
    assert_eq!(event.kind, LimitKind::UsageLimit);
    assert_eq!(event.session_id.as_deref(), Some(SESSION));
    assert_eq!(event.resets_at, None);
}

/// `None` means "not a limit": no wait and no retry can fix it, so the machine
/// must be told nothing at all rather than told to back off.
#[test]
fn every_documented_error_type_maps_to_its_own_kind() {
    let cases = [
        ("rate_limit", Some(LimitKind::UsageLimit)),
        ("overloaded", Some(LimitKind::Transient)),
        ("server_error", Some(LimitKind::Transient)),
        ("authentication_failed", Some(LimitKind::NeedsHuman)),
        ("oauth_org_not_allowed", Some(LimitKind::NeedsHuman)),
        ("billing_error", Some(LimitKind::NeedsHuman)),
        ("invalid_request", None),
        ("model_not_found", None),
        ("max_output_tokens", None),
        ("unknown", None),
        ("an_error_type_from_a_future_release", None),
    ];
    for (error_type, want) in cases {
        let mut claude = Claude::default();
        let got = claude
            .on_signal(&ctx(1), &stop_failure_of(error_type), NOW)
            .map(|event| event.kind);
        assert_eq!(got, want, "{error_type}");
    }
}

#[test]
fn a_stop_failure_without_an_error_type_reports_nothing() {
    let mut claude = Claude::default();
    let signal = stop_failure(json!({"hook_event_name": "StopFailure", "session_id": SESSION}));
    assert_eq!(claude.on_signal(&ctx(1), &signal, NOW), None);
}

#[test]
fn a_payload_naming_a_different_hook_event_is_ignored() {
    let mut claude = Claude::default();
    let signal = stop_failure(json!({
        "hook_event_name": "Stop",
        "session_id": SESSION,
        "error_type": "rate_limit",
    }));
    assert_eq!(claude.on_signal(&ctx(1), &signal, NOW), None);
}

#[test]
fn a_stop_failure_detail_never_repeats_the_error_message() {
    let mut claude = Claude::default();
    let event = claude
        .on_signal(&ctx(1), &stop_failure_of("rate_limit"), NOW)
        .expect("rate_limit is a limit");
    assert!(!event.detail.contains("acct_secret_42"), "{}", event.detail);
    assert!(event.detail.contains("rate_limit"), "{}", event.detail);
}

#[test]
fn a_stop_failure_prefers_the_remembered_statusline_reset() {
    let mut claude = Claude::default();
    let payload = json!({"five_hour": {"resets_at": FIVE_HOUR_RESET}});
    assert_eq!(claude.on_signal(&ctx(1), &rate_limits(payload), NOW), None);
    let event = claude
        .on_signal(&ctx(1), &stop_failure_of("rate_limit"), NOW)
        .expect("rate_limit is a limit");
    assert_eq!(event.resets_at, Some(FIVE_HOUR_RESET));
}

/// Still a limit, just not resumable: a rejected id must leave the event without
/// one so the machine holds rather than resuming some other session.
#[test]
fn a_session_id_that_is_absent_or_unsafe_as_an_argument_is_rejected() {
    let over_long = json!("x".repeat(MAX_SESSION_ID_BYTES + 1));
    let bad = [
        None,
        Some(Value::Null),
        Some(json!("")),
        Some(json!("abc; rm -rf /")),
        Some(json!("abc def")),
        Some(json!("abc$(id)")),
        Some(json!(42)),
        Some(over_long),
    ];
    for id in bad {
        let mut payload = json!({"hook_event_name": "StopFailure", "error_type": "rate_limit"});
        if let Some(id) = id.clone() {
            payload["session_id"] = id;
        }
        let mut claude = Claude::default();
        let event = claude
            .on_signal(&ctx(1), &stop_failure(payload), NOW)
            .expect("a rate limit is still reported");
        assert_eq!(event.session_id, None, "{id:?}");
        let plan = claude.resume(&ctx(1), &event, false);
        assert_eq!(plan, Some(ResumePlan::Hold(NO_SESSION_HOLD)), "{id:?}");
    }
}
