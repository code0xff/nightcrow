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

#[cfg(windows)]
#[test]
fn windows_executable_paths_and_wrapper_shims_are_recognised() {
    for (command, name) in [
        (r"C:\Tools\claude.exe --model opus", "claude"),
        (r#""C:\Program Files\OpenAI\codex.cmd" resume"#, "codex"),
        (r"C:\Tools\opencode.PS1", "opencode"),
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
        Some(r#""C:\Tools\claude.exe"suffix"#),
        Some(r#""C:\Tools\claude.exe"#),
    ] {
        assert!(
            detect(command).is_none(),
            "{command:?} must not be adopted by any adapter"
        );
    }
}

#[test]
fn every_signal_kind_names_the_adapter_whose_helper_minted_it() {
    // A pane with no command line of its own — the shell somebody typed `claude`
    // into — has only the signal to go on, and the signal's kind is enough: each
    // one is written by exactly one provider's helper.
    for kind in [SignalKind::StopFailure, SignalKind::RateLimits] {
        let provider =
            detect_from_signal(kind).unwrap_or_else(|| panic!("{kind:?} names an adapter"));
        assert_eq!(provider.name(), "claude");
    }
}

#[test]
fn a_signal_binds_an_adapter_where_the_command_line_cannot() {
    // The pair that makes the late-adoption path work at all: `detect` gives up
    // on a pane with no command, and the signal is what answers instead.
    assert!(detect(None).is_none());
    assert!(detect_from_signal(SignalKind::StopFailure).is_some());
}

#[test]
fn a_signal_kind_round_trips_through_its_wire_name() {
    for kind in [SignalKind::StopFailure, SignalKind::RateLimits] {
        assert_eq!(SignalKind::from_wire(kind.as_wire()), Some(kind));
    }
    assert_eq!(SignalKind::from_wire("transcript"), None);
    assert_eq!(SignalKind::from_wire(""), None);
}
