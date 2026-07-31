use super::*;
use tempfile::TempDir;

/// A path containing [`MARKER`], as the installed binary's path does.
const EXE: &str = "/opt/nightcrow/libexec/nightcrow-recovery";

fn home() -> (TempDir, SettingsPaths) {
    let dir = TempDir::new().unwrap();
    let paths = SettingsPaths::from_home(dir.path());
    (dir, paths)
}

fn write_settings(paths: &SettingsPaths, text: &str) {
    fs::create_dir_all(paths.settings.parent().unwrap()).unwrap();
    fs::write(&paths.settings, text).unwrap();
}

fn read_value(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn hook_cmd() -> String {
    format!("{EXE} hook")
}

fn statusline_cmd() -> String {
    format!("{EXE} statusline")
}

#[test]
fn installing_with_no_settings_file_creates_one_holding_only_our_entries() {
    let (_dir, paths) = home();

    let changes = install(&paths, EXE).unwrap();

    assert!(!changes.is_empty());
    assert_eq!(
        read_value(&paths.settings),
        json!({
            "hooks": {
                "StopFailure": [{
                    "matcher": "rate_limit",
                    "hooks": [{ "type": "command", "command": hook_cmd(), "timeout": 5 }],
                }],
            },
            "statusLine": { "type": "command", "command": statusline_cmd(), "padding": 2 },
        })
    );
}

#[test]
fn installing_into_a_settings_file_preserves_unknown_keys() {
    let (_dir, paths) = home();
    write_settings(
        &paths,
        r#"{
            "model": "opus",
            "hooks": {
                "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "audit.sh" }] }],
                "someFutureKey": { "kept": true }
            }
        }"#,
    );

    install(&paths, EXE).unwrap();

    let after = read_value(&paths.settings);
    assert_eq!(after["model"], json!("opus"));
    assert_eq!(
        after["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        json!("audit.sh")
    );
    assert_eq!(after["hooks"]["someFutureKey"], json!({ "kept": true }));
    assert_eq!(
        after["hooks"]["StopFailure"][0]["hooks"][0]["command"],
        json!(hook_cmd())
    );
}

#[test]
fn installing_beside_someone_elses_stop_failure_hook_keeps_both() {
    let (_dir, paths) = home();
    write_settings(
        &paths,
        r#"{"hooks":{"StopFailure":[{"matcher":"rate_limit","hooks":[
            {"type":"command","command":"notify-send limit"}]}]}}"#,
    );

    install(&paths, EXE).unwrap();

    let entries = read_value(&paths.settings)["hooks"]["StopFailure"][0]["hooks"].clone();
    let commands: Vec<&str> = entries
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["command"].as_str().unwrap())
        .collect();
    assert_eq!(commands, vec!["notify-send limit", hook_cmd().as_str()]);
}

#[test]
fn installing_twice_changes_nothing_the_second_time_and_reports_so() {
    let (_dir, paths) = home();
    install(&paths, EXE).unwrap();
    let first = read_value(&paths.settings);

    let changes = install(&paths, EXE).unwrap();

    assert_eq!(changes.len(), 1);
    assert!(changes[0].contains("nothing changed"), "{}", changes[0]);
    assert_eq!(read_value(&paths.settings), first);
}

#[test]
fn uninstalling_after_installing_restores_the_original_settings() {
    let (_dir, paths) = home();
    let original = r#"{
        "model": "opus",
        "statusLine": { "type": "command", "command": "~/.claude/mine.sh", "padding": 0 },
        "hooks": { "PreToolUse": [{ "matcher": "", "hooks": [{ "type": "command", "command": "audit.sh" }] }] }
    }"#;
    write_settings(&paths, original);
    install(&paths, EXE).unwrap();

    let changes = uninstall(&paths).unwrap();

    assert!(!changes.is_empty());
    assert_eq!(
        read_value(&paths.settings),
        serde_json::from_str::<Value>(original).unwrap()
    );
    assert!(!paths.sidecar.exists());
}

#[test]
fn uninstalling_leaves_a_statusline_the_user_replaced_alone() {
    let (_dir, paths) = home();
    install(&paths, EXE).unwrap();
    let mut settings = read_value(&paths.settings);
    settings["statusLine"] = json!({ "type": "command", "command": "theirs.sh" });
    write_settings(&paths, &settings.to_string());

    uninstall(&paths).unwrap();

    let after = read_value(&paths.settings);
    assert_eq!(
        after["statusLine"],
        json!({ "type": "command", "command": "theirs.sh" })
    );
    assert_eq!(after.get("hooks"), None, "our hook should still be gone");
}

#[test]
fn uninstalling_with_no_settings_file_is_not_an_error() {
    let (_dir, paths) = home();

    let changes = uninstall(&paths).unwrap();

    assert_eq!(changes.len(), 1);
    assert!(changes[0].contains("nothing to remove"), "{}", changes[0]);
}

#[test]
fn uninstalling_without_a_sidecar_removes_our_statusline_and_invents_none() {
    let (_dir, paths) = home();
    install(&paths, EXE).unwrap();
    fs::remove_file(&paths.sidecar).unwrap();

    uninstall(&paths).unwrap();

    assert_eq!(read_value(&paths.settings), json!({}));
}

#[test]
fn uninstalling_when_nothing_of_ours_is_present_reports_and_writes_nothing() {
    let (_dir, paths) = home();
    let original = r#"{"statusLine":{"type":"command","command":"theirs.sh"}}"#;
    write_settings(&paths, original);

    let changes = uninstall(&paths).unwrap();

    assert_eq!(changes.len(), 1);
    assert!(changes[0].contains("nothing to remove"), "{}", changes[0]);
    assert_eq!(fs::read_to_string(&paths.settings).unwrap(), original);
}

#[test]
fn installing_over_a_json_array_is_refused_and_the_file_is_untouched() {
    assert_refused(r#"[{"statusLine":1}]"#, "array");
}

#[test]
fn installing_over_a_json_scalar_is_refused_and_the_file_is_untouched() {
    assert_refused("42", "number");
}

#[test]
fn installing_over_malformed_json_is_refused_and_the_file_is_untouched() {
    assert_refused("{ this is not json", "not valid JSON");
}

fn assert_refused(original: &str, expected_in_error: &str) {
    let (_dir, paths) = home();
    write_settings(&paths, original);

    let error = install(&paths, EXE).unwrap_err().to_string();

    assert!(error.contains(expected_in_error), "{error}");
    assert!(
        error.contains(&paths.settings.display().to_string()),
        "error must name the file: {error}"
    );
    assert_eq!(fs::read_to_string(&paths.settings).unwrap(), original);
    assert!(!paths.backup.exists());
    assert!(!paths.sidecar.exists());
}

#[test]
#[cfg(unix)]
fn installing_backs_up_the_previous_file_and_writes_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, paths) = home();
    let original = r#"{"model":"opus"}"#;
    write_settings(&paths, original);

    install(&paths, EXE).unwrap();

    assert_eq!(fs::read_to_string(&paths.backup).unwrap(), original);
    let mode = fs::metadata(&paths.settings).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "mode was {mode:o}");
}

#[test]
fn installing_without_a_settings_file_writes_no_backup() {
    let (_dir, paths) = home();

    install(&paths, EXE).unwrap();

    assert!(!paths.backup.exists());
}

#[test]
fn installing_through_a_binary_without_the_marker_is_refused() {
    let (_dir, paths) = home();

    let error = install(&paths, "/usr/bin/renamed").unwrap_err().to_string();

    assert!(error.contains(MARKER), "{error}");
    assert!(!paths.settings.exists());
}

#[test]
fn settings_paths_from_home_composes_the_documented_paths() {
    let paths = SettingsPaths::from_home(Path::new("/home/dev"));

    assert_eq!(
        paths.settings,
        PathBuf::from("/home/dev/.claude/settings.json")
    );
    assert_eq!(
        paths.backup,
        PathBuf::from("/home/dev/.claude/settings.json.bak")
    );
    assert_eq!(
        paths.sidecar,
        PathBuf::from("/home/dev/.claude/nightcrow-recovery.displaced.json")
    );
}
