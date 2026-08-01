use super::json;
use crate::git::diff::{ChangedFile, StatusKind};
use crate::web::viewer::dto::{CommitFilesDto, StatusDto};
use crate::web::viewer::limits;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

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
fn status_dto_carries_mtime_only_for_stated_files() {
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
    let mtimes = HashMap::from([(
        "hot.rs".to_string(),
        SystemTime::UNIX_EPOCH + Duration::from_millis(1_500),
    )]);

    let value = json(&StatusDto::from_snapshot(&files, None, None, None, &mtimes));

    assert_eq!(value["files"][0]["mtime"], 1_500u64);
    assert!(value["files"][1].get("mtime").is_none());
}

#[test]
fn commit_file_list_never_carries_an_mtime() {
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
