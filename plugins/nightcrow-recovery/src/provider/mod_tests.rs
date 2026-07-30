use super::*;

/// A readable fixed "now": 2026-01-01T00:00:00Z.
const NOW: i64 = 1_767_225_600;

#[test]
fn a_reset_time_in_the_documented_shape_is_read_as_unix_seconds() {
    let payload = serde_json::json!({"five_hour": {"resets_at": NOW + 3600}});
    assert_eq!(
        reset_epoch_from_json(&payload, &["five_hour", "resets_at"], NOW),
        Some(NOW + 3600)
    );
}

#[test]
fn a_reset_time_that_is_absent_is_not_invented() {
    let payload = serde_json::json!({"five_hour": {}});
    assert_eq!(
        reset_epoch_from_json(&payload, &["five_hour", "resets_at"], NOW),
        None
    );
    assert_eq!(reset_epoch_from_json(&payload, &["seven_day"], NOW), None);
}

#[test]
fn a_reset_time_of_the_wrong_type_is_refused_rather_than_coerced() {
    for value in [
        serde_json::json!("1767225600"),
        serde_json::json!(null),
        serde_json::json!(1.5),
        serde_json::json!([NOW]),
        serde_json::json!({"secs": NOW}),
    ] {
        let payload = serde_json::json!({"w": {"resets_at": value}});
        assert_eq!(
            reset_epoch_from_json(&payload, &["w", "resets_at"], NOW),
            None,
            "{value:?} is not a unix second"
        );
    }
}

#[test]
fn a_reset_time_outside_the_plausible_band_is_treated_as_unknown() {
    assert_eq!(plausible_reset(0, NOW), None);
    assert_eq!(plausible_reset(-1, NOW), None);
    // A millisecond timestamp is far past the horizon, not a reset time.
    assert_eq!(plausible_reset(NOW * 1000, NOW), None);
    assert_eq!(plausible_reset(NOW + MAX_RESET_HORIZON_SECS + 1, NOW), None);
    assert_eq!(
        plausible_reset(NOW + MAX_RESET_HORIZON_SECS, NOW),
        Some(NOW + MAX_RESET_HORIZON_SECS)
    );
}

#[test]
fn a_reset_time_already_in_the_past_is_still_a_reset_time() {
    // Stale, not implausible: the state machine's minimum wait handles it, and
    // discarding it here would lose the only deadline we were given.
    assert_eq!(plausible_reset(NOW - 60, NOW), Some(NOW - 60));
}

#[test]
fn each_known_provider_is_recognised_from_its_command_line() {
    for (command, name) in [
        ("claude", "claude"),
        ("claude --model opus", "claude"),
        ("/usr/local/bin/claude", "claude"),
        ("codex", "codex"),
        ("codex resume --last", "codex"),
        ("opencode", "opencode"),
        ("  opencode --port 5000", "opencode"),
    ] {
        let provider = detect(Some(command)).unwrap_or_else(|| panic!("{command} is known"));
        assert_eq!(provider.name(), name);
    }
}

#[test]
fn a_pane_running_something_else_is_not_watched_at_all() {
    for command in [
        None,
        Some(""),
        Some("   "),
        Some("bash"),
        Some("zsh -l"),
        Some("claudette"),
    ] {
        assert!(
            detect(command).is_none(),
            "{command:?} must not be adopted by any adapter"
        );
    }
}

#[test]
fn a_signal_kind_round_trips_through_its_wire_name() {
    for kind in [SignalKind::StopFailure, SignalKind::RateLimits] {
        assert_eq!(SignalKind::from_wire(kind.as_wire()), Some(kind));
    }
    assert_eq!(SignalKind::from_wire("transcript"), None);
    assert_eq!(SignalKind::from_wire(""), None);
}
