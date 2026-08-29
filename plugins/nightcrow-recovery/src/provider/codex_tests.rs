//! Rollout-tailing tests, against real `CODEX_HOME` trees under a `TempDir`.
//!
//! The exhaustive record-grammar cases (every bad `resets_at` shape, every file
//! name shape) live in `codex_rollout_tests.rs`, next to the parser that decides
//! them; terminal-output and resume cases live in `codex_output_tests.rs`.

use super::rollout::USAGE_LIMIT_ERROR_INFO;
use super::*;
use crate::protocol::PaneGeneration;
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

const UUID_A: &str = "0199cbb1-2b70-7f11-9f0f-0f8e9d1c2b3a";
const UUID_B: &str = "0199cbb1-2b70-7f11-9f0f-aaaabbbbcccc";
/// One local day directory. The adapter takes the newest two that exist, so only
/// the shape has to match codex's.
const DAY_DIR: &str = "sessions/2026/07/30";
/// Watching starts a minute in the past, so a rollout a test just wrote counts as
/// modified at or after the watch start.
const WATCH_SLACK_SECS: i64 = 60;
/// A deadline inside the accepted horizon.
const RESET_AHEAD_SECS: i64 = 3_600;

fn now() -> i64 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    i64::try_from(secs).unwrap_or(0)
}

fn watch_now() -> i64 {
    now() - WATCH_SLACK_SECS
}

fn ctx(generation: PaneGeneration) -> PaneContext {
    PaneContext {
        token: "pane-0".to_string(),
        generation,
        cwd: "/repo".to_string(),
        command: Some("codex".to_string()),
    }
}

fn record(tag: &str, payload: Value) -> String {
    json!({"timestamp":"2026-07-30T19:23:45Z","ordinal":1,"type":tag,"payload":payload}).to_string()
}

fn meta(id: Option<&str>) -> String {
    let payload = match id {
        Some(id) => json!({"id": id}),
        None => json!({}),
    };
    record("session_meta", payload)
}

fn token_count(resets_at: Value) -> String {
    record("token_count", rate_limits(json!({"resets_at": resets_at})))
}

fn rate_limits(primary: Value) -> Value {
    json!({"rate_limits": {"primary": primary}})
}

fn limit_turn() -> String {
    record(
        "turn_complete",
        json!({"error": {"codex_error_info": USAGE_LIMIT_ERROR_INFO}}),
    )
}

fn joined(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn write_rollout(home: &Path, uuid: &str, lines: &[String]) -> PathBuf {
    let dir = home.join(DAY_DIR);
    std::fs::create_dir_all(&dir).expect("create day directory");
    let path = dir.join(format!("rollout-2026-07-30T19-23-45-{uuid}.jsonl"));
    std::fs::write(&path, joined(lines)).expect("write rollout");
    path
}

fn append(path: &Path, lines: &[String]) {
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open rollout for append");
    file.write_all(joined(lines).as_bytes())
        .expect("append rollout lines");
}

/// A home holding one rollout with `lines`, plus the result of one poll. The
/// `TempDir` is returned because dropping it would delete the tree.
fn poll_once(lines: &[String]) -> (TempDir, Option<LimitEvent>) {
    let home = TempDir::new().expect("temp home");
    write_rollout(home.path(), UUID_A, lines);
    let mut codex = Codex::with_home(home.path().to_path_buf());
    let event = codex.poll(&ctx(1), watch_now());
    (home, event)
}

/// The deadline a usage limit carries when `payload` was the `token_count` before
/// it.
fn deadline_for(payload: Value) -> Option<i64> {
    let lines = [
        meta(Some(UUID_A)),
        record("token_count", payload),
        limit_turn(),
    ];
    let (_home, event) = poll_once(&lines);
    event.expect("a usage limit event").resets_at
}

#[test]
fn a_token_count_with_a_valid_resets_at_supplies_the_event_deadline() {
    let reset = now() + RESET_AHEAD_SECS;
    assert_eq!(
        deadline_for(rate_limits(json!({"resets_at": reset}))),
        Some(reset)
    );
}

#[test]
fn a_resets_at_that_is_null_leaves_the_deadline_unknown() {
    assert_eq!(deadline_for(rate_limits(json!({"resets_at": null}))), None);
}

#[test]
fn a_malformed_line_is_skipped_and_later_records_still_parse() {
    let reset = now() + RESET_AHEAD_SECS;
    let lines = [
        meta(Some(UUID_A)),
        "{\"type\":\"token_count\",".to_string(),
        token_count(json!(reset)),
        limit_turn(),
    ];
    let (_home, event) = poll_once(&lines);
    let event = event.expect("a usage limit event");
    assert_eq!(event.resets_at, Some(reset));
    assert_eq!(event.detail, USAGE_LIMIT_ERROR_INFO);
    assert_eq!(event.session_id.as_deref(), Some(UUID_A));
}

#[test]
fn a_turn_complete_with_another_error_emits_nothing() {
    let other = record(
        "turn_complete",
        json!({"error": {"codex_error_info": "context_length_exceeded"}}),
    );
    let (_home, event) = poll_once(&[meta(Some(UUID_A)), other]);
    assert_eq!(event, None);
}

#[test]
fn a_turn_complete_without_an_error_emits_nothing() {
    let clean = record("turn_complete", json!({"usage": {"input_tokens": 1}}));
    let (_home, event) = poll_once(&[meta(Some(UUID_A)), clean]);
    assert_eq!(event, None);
}

#[test]
fn a_token_count_arriving_after_the_turn_complete_does_not_fire_a_second_event() {
    let reset = now() + RESET_AHEAD_SECS;
    let home = TempDir::new().expect("temp home");
    let path = write_rollout(
        home.path(),
        UUID_A,
        &[meta(Some(UUID_A)), token_count(json!(reset)), limit_turn()],
    );
    let mut codex = Codex::with_home(home.path().to_path_buf());
    assert!(codex.poll(&ctx(1), watch_now()).is_some());
    append(&path, &[token_count(json!(reset + RESET_AHEAD_SECS))]);
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
}

#[test]
fn two_rollout_files_and_no_binding_stay_ambiguous_so_resume_holds() {
    let home = TempDir::new().expect("temp home");
    let lines = [meta(Some(UUID_A)), limit_turn()];
    write_rollout(home.path(), UUID_A, &lines);
    write_rollout(home.path(), UUID_B, &lines);
    let mut codex = Codex::with_home(home.path().to_path_buf());
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
    // Still ambiguous on a later tick: binding late is no safer than binding now.
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
    let limit = LimitEvent::usage(None, None, USAGE_LIMIT_ERROR_INFO);
    assert_eq!(
        codex.resume(&ctx(1), &limit, false),
        Some(ResumePlan::Hold(HOLD_NO_ID))
    );
}

#[test]
fn a_single_rollout_file_binds_and_resume_asks_for_exactly_resume_and_the_id() {
    let home = TempDir::new().expect("temp home");
    write_rollout(home.path(), UUID_A, &[meta(Some(UUID_A)), limit_turn()]);
    let mut codex = Codex::with_home(home.path().to_path_buf());
    let event = codex
        .poll(&ctx(1), watch_now())
        .expect("a usage limit event");
    assert_eq!(event.session_id.as_deref(), Some(UUID_A));
    assert_eq!(
        codex.resume(&ctx(1), &event, false),
        Some(ResumePlan::Relaunch(vec![
            "resume".to_string(),
            UUID_A.to_string()
        ]))
    );
}

#[test]
fn a_session_meta_without_an_id_falls_back_to_the_filename_uuid() {
    let (_home, event) = poll_once(&[meta(None), limit_turn()]);
    let event = event.expect("a usage limit event");
    assert_eq!(event.session_id.as_deref(), Some(UUID_A));
}

#[test]
fn appended_lines_are_read_incrementally_and_a_consumed_record_does_not_fire_twice() {
    let reset = now() + RESET_AHEAD_SECS;
    let home = TempDir::new().expect("temp home");
    let path = write_rollout(
        home.path(),
        UUID_A,
        &[meta(Some(UUID_A)), token_count(json!(reset))],
    );
    let mut codex = Codex::with_home(home.path().to_path_buf());
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
    append(&path, &[limit_turn()]);
    let event = codex
        .poll(&ctx(1), watch_now())
        .expect("a usage limit event");
    assert_eq!(event.resets_at, Some(reset));
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
}

#[test]
fn a_truncated_rollout_file_is_re_read_from_the_start() {
    let home = TempDir::new().expect("temp home");
    let path = write_rollout(
        home.path(),
        UUID_A,
        &[meta(Some(UUID_A)), token_count(json!(null)), limit_turn()],
    );
    let mut codex = Codex::with_home(home.path().to_path_buf());
    assert!(codex.poll(&ctx(1), watch_now()).is_some());
    std::fs::write(&path, joined(&[limit_turn()])).expect("truncate rollout");
    let event = codex
        .poll(&ctx(1), watch_now())
        .expect("the rewritten record fires again");
    assert_eq!(event.session_id.as_deref(), Some(UUID_A));
}

#[test]
fn a_partial_trailing_line_is_only_applied_once_it_is_complete() {
    let home = TempDir::new().expect("temp home");
    let path = write_rollout(home.path(), UUID_A, &[meta(Some(UUID_A))]);
    let mut codex = Codex::with_home(home.path().to_path_buf());
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
    let whole = limit_turn();
    let (head, tail) = whole.split_at(whole.len() / 2);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open rollout for append");
    file.write_all(head.as_bytes()).expect("write head");
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
    file.write_all(format!("{tail}\n").as_bytes())
        .expect("write tail");
    assert!(codex.poll(&ctx(1), watch_now()).is_some());
}

#[test]
fn a_missing_codex_home_never_fires_and_never_panics() {
    let home = TempDir::new().expect("temp home");
    let mut codex = Codex::with_home(home.path().join("never-created"));
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
    assert_eq!(codex.poll(&ctx(1), watch_now()), None);
}

#[test]
fn a_rollout_older_than_the_watch_start_is_not_bound() {
    let home = TempDir::new().expect("temp home");
    write_rollout(home.path(), UUID_A, &[meta(Some(UUID_A)), limit_turn()]);
    let mut codex = Codex::with_home(home.path().to_path_buf());
    // Watching starts in the future, so nothing already on disk qualifies.
    assert_eq!(codex.poll(&ctx(1), now() + RESET_AHEAD_SECS), None);
}
