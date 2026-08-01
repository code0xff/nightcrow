use super::json;
use crate::git::diff::{ChangedFile, CommitEntry, StatusKind};
use crate::web::viewer::dto::{ChangedFileDto, CommitDto, Envelope, PROTOCOL_VERSION, TreeDto};

#[test]
fn changed_file_dto_drops_the_tui_search_cache() {
    let file = ChangedFile::from_status_columns(
        "src/main.rs".to_string(),
        None,
        StatusKind::Modified,
        StatusKind::Unmodified,
    );
    assert!(!file.search_lower.is_empty(), "precondition");

    let value = json(&ChangedFileDto::from(&file));

    assert_eq!(value["path"], "src/main.rs");
    assert_eq!(value["index"], "M");
    assert_eq!(value["worktree"], " ");
    assert!(value.get("search_lower").is_none());
    assert!(value.get("old_path").is_none());
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
    assert!(value.get("summary_lower").is_none());
}

#[test]
fn envelope_carries_the_protocol_version_alongside_the_payload() {
    let value = json(&Envelope::new(TreeDto::from_entries("src", &[])));

    assert_eq!(value["version"], PROTOCOL_VERSION);
    assert_eq!(value["path"], "src", "the payload is flattened");
}
