use super::*;

/// A path containing [`MARKER`], as the installed binary's path does.
const EXE: &str = "/opt/nightcrow/libexec/nightcrow-recovery";

fn hook_cmd() -> String {
    format!("'{EXE}' hook")
}

fn our_group(settings: &Value) -> &Value {
    &settings[HOOKS_KEY][HOOK_EVENT][0]
}

#[test]
fn merging_into_an_empty_object_adds_both_hooks_and_a_statusline() {
    let mut settings = json!({});

    let (changes, displaced) = merge_into(&mut settings, EXE).unwrap();

    assert_eq!(changes.len(), 3, "{changes:?}");
    assert_eq!(displaced, Some(Value::Null));
    assert_eq!(our_group(&settings)["matcher"], json!(HOOK_MATCHER));
    assert_eq!(
        settings[HOOKS_KEY][TURN_END_EVENT][0]["matcher"],
        json!(TURN_END_MATCHER)
    );
    assert_eq!(
        settings[HOOKS_KEY][TURN_END_EVENT][0][HOOKS_KEY][0]["command"],
        json!(format!("'{EXE}' turn-end"))
    );
    assert_eq!(
        settings[STATUSLINE_KEY]["command"],
        json!(format!("'{EXE}' statusline"))
    );
}

#[test]
fn merging_keeps_unknown_nested_keys_and_unrelated_hook_events() {
    let mut settings = json!({
        "unknownTopLevel": [1, 2, 3],
        "hooks": {
            "unknownNested": "kept",
            "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "audit.sh" }] }]
        }
    });

    merge_into(&mut settings, EXE).unwrap();

    assert_eq!(settings["unknownTopLevel"], json!([1, 2, 3]));
    assert_eq!(settings["hooks"]["unknownNested"], json!("kept"));
    assert_eq!(
        settings["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        json!("audit.sh")
    );
}

#[test]
fn merging_reuses_a_matcher_group_that_already_exists() {
    let mut settings = json!({
        "hooks": { "StopFailure": [
            { "matcher": "billing_error", "hooks": [{ "type": "command", "command": "page-me" }] },
            { "matcher": "rate_limit", "hooks": [{ "type": "command", "command": "theirs" }] }
        ]}
    });

    merge_into(&mut settings, EXE).unwrap();

    let groups = settings["hooks"]["StopFailure"].as_array().unwrap();
    assert_eq!(groups.len(), 2, "no new group should be appended");
    assert_eq!(groups[1]["hooks"][0]["command"], json!("theirs"));
    assert_eq!(groups[1]["hooks"][1]["command"], json!(hook_cmd()));
}

#[test]
fn merging_twice_reports_no_change_the_second_time() {
    let mut settings = json!({});
    merge_into(&mut settings, EXE).unwrap();
    let after_first = settings.clone();

    let (changes, displaced) = merge_into(&mut settings, EXE).unwrap();

    assert!(changes.is_empty(), "{changes:?}");
    assert_eq!(displaced, None);
    assert_eq!(settings, after_first);
}

#[test]
fn merging_over_a_statusline_of_our_own_leaves_it_and_records_nothing() {
    let mut settings = json!({
        "statusLine": { "type": "command", "command": format!("{EXE} statusline --wide") }
    });
    let before = settings[STATUSLINE_KEY].clone();

    let (_, displaced) = merge_into(&mut settings, EXE).unwrap();

    assert_eq!(displaced, None);
    assert_eq!(settings[STATUSLINE_KEY], before);
}

#[test]
fn merging_when_hooks_is_not_an_object_is_refused_and_names_the_field() {
    let mut settings = json!({ "hooks": "surprise" });

    let error = merge_into(&mut settings, EXE).unwrap_err().to_string();

    assert!(error.contains("`hooks`"), "{error}");
    assert!(error.contains("string"), "{error}");
}

#[test]
fn merging_when_the_stop_failure_event_is_not_an_array_is_refused() {
    let mut settings = json!({ "hooks": { "StopFailure": { "matcher": "" } } });

    let error = merge_into(&mut settings, EXE).unwrap_err().to_string();

    assert!(error.contains("`hooks.StopFailure`"), "{error}");
}

#[test]
fn stripping_what_was_merged_returns_the_settings_to_their_original_shape() {
    let original = json!({
        "model": "opus",
        "hooks": { "PreToolUse": [{ "matcher": "", "hooks": [{ "type": "command", "command": "audit.sh" }] }] }
    });
    let mut settings = original.clone();
    let (_, displaced) = merge_into(&mut settings, EXE).unwrap();

    let changes = strip_from(&mut settings, displaced).unwrap();

    assert_eq!(changes.len(), 2, "{changes:?}");
    assert_eq!(settings, original);
}

#[test]
fn stripping_drops_the_whole_hooks_subtree_when_it_only_held_our_entry() {
    let mut settings = json!({ "model": "opus" });
    let (_, displaced) = merge_into(&mut settings, EXE).unwrap();

    strip_from(&mut settings, displaced).unwrap();

    assert_eq!(settings, json!({ "model": "opus" }));
}

#[test]
fn stripping_restores_the_recorded_statusline() {
    let theirs = json!({ "type": "command", "command": "mine.sh", "refreshInterval": 1 });
    let mut settings = json!({ "statusLine": theirs.clone() });
    let (_, displaced) = merge_into(&mut settings, EXE).unwrap();

    strip_from(&mut settings, displaced).unwrap();

    assert_eq!(settings[STATUSLINE_KEY], theirs);
}

#[test]
fn stripping_without_a_recorded_value_removes_our_statusline() {
    let mut settings = json!({});
    merge_into(&mut settings, EXE).unwrap();

    strip_from(&mut settings, None).unwrap();

    assert_eq!(settings.get(STATUSLINE_KEY), None);
}

#[test]
fn stripping_leaves_a_foreign_hook_command_and_its_group_in_place() {
    let original = json!({
        "hooks": { "StopFailure": [
            { "matcher": "rate_limit", "hooks": [{ "type": "command", "command": "theirs" }] }
        ]},
        "statusLine": { "type": "command", "command": "theirs.sh" }
    });
    let mut settings = original.clone();

    let changes = strip_from(&mut settings, Some(json!("bogus"))).unwrap();

    assert!(changes.is_empty(), "{changes:?}");
    assert_eq!(settings, original);
}

#[test]
fn stripping_keeps_a_users_own_empty_stop_failure_list() {
    let original = json!({ "hooks": { "StopFailure": [] } });
    let mut settings = original.clone();

    strip_from(&mut settings, None).unwrap();

    assert_eq!(settings, original);
}

#[test]
fn stripping_a_hooks_subtree_of_an_unexpected_shape_changes_nothing() {
    let original = json!({ "hooks": "surprise" });
    let mut settings = original.clone();

    let changes = strip_from(&mut settings, None).unwrap();

    assert!(changes.is_empty(), "{changes:?}");
    assert_eq!(settings, original);
}

#[test]
fn stripping_a_non_object_root_is_refused() {
    let mut settings = json!([1, 2]);

    let error = strip_from(&mut settings, None).unwrap_err().to_string();

    assert!(error.contains("settings root"), "{error}");
    assert!(error.contains("array"), "{error}");
}

#[test]
fn stripping_removes_the_turn_end_hook_as_well() {
    let mut settings = json!({});
    merge_into(&mut settings, EXE).unwrap();

    strip_from(&mut settings, Some(Value::Null)).unwrap();

    assert_eq!(settings.get(HOOKS_KEY), None, "both events went with it");
}

/// The bug that cost a user their statusline and both hooks: an unquoted
/// Windows path reaches the shell with every backslash eaten.
#[test]
fn a_windows_path_is_quoted_so_the_shell_keeps_its_separators() {
    let exe = r"C:\Users\me\.nightcrow\plugins\nightcrow-recovery";

    let command = hook_command(exe);

    assert_eq!(command, format!("'{exe}' hook"));
    assert!(is_ours(&command), "quoting must not hide our marker");
}

#[test]
fn a_quote_in_the_path_is_escaped_rather_than_ending_the_quoting() {
    assert_eq!(
        hook_command("/opt/it's/nightcrow-recovery"),
        r"'/opt/it'\''s/nightcrow-recovery' hook"
    );
}
