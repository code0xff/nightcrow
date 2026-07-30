mod fixture;

use super::*;
use crate::git::diff::{ChangedFile, CommitEntry, DiffHunk, LineKind, StatusKind};
use crate::web::viewer::limits;
use serde::Serialize;
use std::collections::HashMap;
use std::time::SystemTime;

fn json<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn changed_file_dto_drops_the_tui_search_cache() {
    let file = ChangedFile::from_status_columns(
        "src/main.rs".to_string(),
        None,
        StatusKind::Modified,
        StatusKind::Unmodified,
    );
    assert!(
        !file.search_lower.is_empty(),
        "precondition: the internal type carries a search cache"
    );

    let value = json(&ChangedFileDto::from(&file));

    assert_eq!(value["path"], "src/main.rs");
    assert_eq!(value["index"], "M");
    assert_eq!(value["worktree"], " ");
    assert!(
        value.get("search_lower").is_none(),
        "the filter cache must not reach the wire: {value}"
    );
    assert!(
        value.get("old_path").is_none(),
        "an absent rename source is omitted, not null"
    );
}

#[test]
fn changed_file_dto_keeps_a_rename_source() {
    let file = ChangedFile::from_status_columns(
        "new.rs".to_string(),
        Some("old.rs".to_string()),
        StatusKind::Renamed,
        StatusKind::Unmodified,
    );

    let value = json(&ChangedFileDto::from(&file));

    assert_eq!(value["old_path"], "old.rs");
    assert_eq!(value["index"], "R");
}

#[test]
fn commit_dto_drops_the_summary_cache_and_hexes_the_oid() {
    let entry = CommitEntry::new(
        git2::Oid::from_str("1234567890abcdef1234567890abcdef12345678").unwrap(),
        "1234567".to_string(),
        "Fix The Bug".to_string(),
        "Someone".to_string(),
        1_700_000_000,
    );
    assert!(!entry.summary_lower.is_empty(), "precondition");

    let value = json(&CommitDto::from(&entry));

    assert_eq!(value["oid"], "1234567890abcdef1234567890abcdef12345678");
    assert_eq!(value["summary"], "Fix The Bug");
    assert!(
        value.get("summary_lower").is_none(),
        "the filter cache must not reach the wire: {value}"
    );
}

#[test]
fn envelope_carries_the_protocol_version_alongside_the_payload() {
    let value = json(&Envelope::new(TreeDto::from_entries("src", &[])));

    assert_eq!(value["version"], PROTOCOL_VERSION);
    assert_eq!(value["path"], "src", "the payload is flattened, not nested");
}

#[test]
fn status_dto_reports_truncation_past_the_ceiling() {
    let files: Vec<_> = (0..limits::MAX_STATUS_FILES + 5)
        .map(|i| {
            ChangedFile::from_status_columns(
                format!("f{i}.rs"),
                None,
                StatusKind::Modified,
                StatusKind::Unmodified,
            )
        })
        .collect();

    let dto = StatusDto::from_snapshot(&files, None, None, Some("main"), &HashMap::new());

    assert_eq!(dto.files.len(), limits::MAX_STATUS_FILES);
    assert!(dto.truncated);
    assert_eq!(dto.branch.as_deref(), Some("main"));
}

#[test]
fn status_dto_carries_mtime_in_millis_only_for_stated_files() {
    let files = vec![
        ChangedFile::from_status_columns(
            "hot.rs".to_string(),
            None,
            StatusKind::Modified,
            StatusKind::Unmodified,
        ),
        ChangedFile::from_status_columns(
            "gone.rs".to_string(),
            None,
            StatusKind::Deleted,
            StatusKind::Unmodified,
        ),
    ];
    // Deleted files never make it into the worker's mtime map, so their
    // rows must simply omit the field rather than carry a stand-in age.
    let mtimes = HashMap::from([(
        "hot.rs".to_string(),
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(1_500),
    )]);

    let value = json(&StatusDto::from_snapshot(&files, None, None, None, &mtimes));

    assert_eq!(value["files"][0]["mtime"], 1_500u64);
    assert!(value["files"][1].get("mtime").is_none());
}

#[test]
fn commit_file_list_never_carries_an_mtime() {
    // A commit's files describe history; the working tree's mtime would be
    // unrelated to them, and a client must not be able to read one as
    // "this commit touched the file just now".
    let files = vec![ChangedFile::from_status_columns(
        "a.rs".to_string(),
        None,
        StatusKind::Modified,
        StatusKind::Unmodified,
    )];

    let value = json(&CommitFilesDto::from_entries(&files));

    assert!(value["files"][0].get("mtime").is_none());
}

#[test]
fn status_dto_omits_absent_optional_fields() {
    let value = json(&StatusDto::from_snapshot(
        &[],
        None,
        None,
        None,
        &HashMap::new(),
    ));

    assert!(value.get("branch").is_none());
    assert!(value.get("head").is_none());
    assert!(value.get("tracking").is_none());
    assert_eq!(value["truncated"], false);
}

fn hunk(header: &str, lines: usize, width: usize) -> DiffHunk {
    DiffHunk {
        header: header.to_string(),
        file_path: None,
        lines: (0..lines)
            .map(|_| crate::git::diff::DiffLine {
                kind: LineKind::Context,
                content: "x".repeat(width),
                old_lineno: None,
                new_lineno: None,
            })
            .collect(),
    }
}

#[test]
fn diff_dto_maps_line_kinds_to_wire_codes() {
    let hunks = vec![DiffHunk {
        header: "@@ -1 +1 @@".to_string(),
        file_path: Some("a.rs".to_string()),
        lines: vec![
            crate::git::diff::DiffLine {
                kind: LineKind::Added,
                content: "new".into(),
                old_lineno: None,
                new_lineno: Some(1),
            },
            crate::git::diff::DiffLine {
                kind: LineKind::Removed,
                content: "old".into(),
                old_lineno: Some(1),
                new_lineno: None,
            },
            crate::git::diff::DiffLine {
                kind: LineKind::Context,
                content: "same".into(),
                old_lineno: Some(2),
                new_lineno: Some(2),
            },
        ],
    }];

    let dto = DiffDto::from_hunks("a.rs", &hunks);

    let kinds: Vec<_> = dto.hunks[0].lines.iter().map(|l| l.kind.as_str()).collect();
    assert_eq!(kinds, vec!["+", "-", " "]);
    assert!(!dto.truncated);
}

#[test]
fn diff_dto_carries_the_line_numbers_of_the_side_each_line_exists_on() {
    let hunks = vec![DiffHunk {
        header: "@@ -1,2 +1,2 @@".to_string(),
        file_path: None,
        lines: vec![
            crate::git::diff::DiffLine {
                kind: LineKind::Added,
                content: "new".into(),
                old_lineno: None,
                new_lineno: Some(1),
            },
            crate::git::diff::DiffLine {
                kind: LineKind::Removed,
                content: "old".into(),
                old_lineno: Some(1),
                new_lineno: None,
            },
            crate::git::diff::DiffLine {
                kind: LineKind::Context,
                content: "same".into(),
                old_lineno: Some(2),
                new_lineno: Some(2),
            },
        ],
    }];

    let dto = DiffDto::from_hunks("a.rs", &hunks);

    let linenos: Vec<_> = dto.hunks[0]
        .lines
        .iter()
        .map(|l| (l.old_lineno, l.new_lineno))
        .collect();
    assert_eq!(
        linenos,
        vec![(None, Some(1)), (Some(1), None), (Some(2), Some(2))]
    );
}

#[test]
fn diff_dto_caps_across_hunks_not_within_one() {
    // Each hunk is under the ceiling alone; together they exceed it. A
    // per-hunk cap would let the total through unbounded.
    let per_hunk = limits::MAX_DIFF_LINES / 2 + 10;
    let hunks = vec![hunk("@@ a @@", per_hunk, 1), hunk("@@ b @@", per_hunk, 1)];

    let dto = DiffDto::from_hunks("big.rs", &hunks);

    let total: usize = dto.hunks.iter().map(|h| h.lines.len()).sum();
    assert_eq!(total, limits::MAX_DIFF_LINES);
    assert!(dto.truncated);
}

#[test]
fn diff_dto_stops_on_the_byte_ceiling_before_the_line_ceiling() {
    // Few lines, each enormous: the byte ceiling has to bind first.
    let hunks = vec![hunk("@@ a @@", 50, limits::MAX_DIFF_BYTES / 10)];

    let dto = DiffDto::from_hunks("wide.rs", &hunks);

    let bytes: usize = dto.hunks[0]
        .lines
        .iter()
        .flat_map(|l| &l.spans)
        .map(|s| s.t.len())
        .sum();
    assert!(bytes <= limits::MAX_DIFF_BYTES);
    assert!(dto.truncated);
    assert!(
        dto.hunks[0].lines.len() < 50,
        "the byte ceiling must cut before the line count does"
    );
}

#[test]
fn file_dto_caps_content_on_a_character_boundary() {
    let content = "한".repeat(limits::MAX_DIFF_BYTES);

    let dto = FileDto::new("big.txt", &content);

    // Reconstruct the served text from its spans (one line here — no \n).
    let served: String = dto.lines.iter().flatten().map(|s| s.t.as_str()).collect();
    assert!(dto.truncated);
    assert!(served.len() <= limits::MAX_DIFF_BYTES);
    assert!(
        content.starts_with(&served),
        "the cap must yield a clean prefix"
    );
}
