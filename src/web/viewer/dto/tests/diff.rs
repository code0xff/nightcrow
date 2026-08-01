use super::json;
use crate::git::diff::{DiffHunk, DiffLine, LineKind};
use crate::web::viewer::dto::{DiffDto, FileDto};
use crate::web::viewer::limits;

fn hunk(header: &str, lines: usize, width: usize) -> DiffHunk {
    DiffHunk {
        header: header.to_string(),
        file_path: None,
        lines: (0..lines)
            .map(|_| DiffLine {
                kind: LineKind::Context,
                content: "x".repeat(width),
                old_lineno: None,
                new_lineno: None,
            })
            .collect(),
    }
}

fn sample_lines() -> Vec<DiffLine> {
    vec![
        DiffLine {
            kind: LineKind::Added,
            content: "new".into(),
            old_lineno: None,
            new_lineno: Some(1),
        },
        DiffLine {
            kind: LineKind::Removed,
            content: "old".into(),
            old_lineno: Some(1),
            new_lineno: None,
        },
        DiffLine {
            kind: LineKind::Context,
            content: "same".into(),
            old_lineno: Some(2),
            new_lineno: Some(2),
        },
    ]
}

#[test]
fn diff_dto_maps_line_kinds_to_wire_codes() {
    let hunks = vec![DiffHunk {
        header: "@@ -1 +1 @@".to_string(),
        file_path: Some("a.rs".to_string()),
        lines: sample_lines(),
    }];

    let dto = DiffDto::from_hunks("a.rs", &hunks);

    let kinds: Vec<_> = dto.hunks[0]
        .lines
        .iter()
        .map(|line| line.kind.as_str())
        .collect();
    assert_eq!(kinds, vec!["+", "-", " "]);
    assert!(!dto.truncated);
}

#[test]
fn diff_dto_carries_the_line_numbers_of_each_side() {
    let hunks = vec![DiffHunk {
        header: "@@ -1,2 +1,2 @@".to_string(),
        file_path: None,
        lines: sample_lines(),
    }];

    let dto = DiffDto::from_hunks("a.rs", &hunks);

    let lines: Vec<_> = dto.hunks[0]
        .lines
        .iter()
        .map(|line| (line.old_lineno, line.new_lineno))
        .collect();
    assert_eq!(
        lines,
        vec![(None, Some(1)), (Some(1), None), (Some(2), Some(2))]
    );
}

#[test]
fn diff_dto_caps_across_hunks_not_within_one() {
    let per_hunk = limits::MAX_DIFF_LINES / 2 + 10;
    let hunks = vec![hunk("@@ a @@", per_hunk, 1), hunk("@@ b @@", per_hunk, 1)];

    let dto = DiffDto::from_hunks("big.rs", &hunks);

    let total: usize = dto.hunks.iter().map(|hunk| hunk.lines.len()).sum();
    assert_eq!(total, limits::MAX_DIFF_LINES);
    assert!(dto.truncated);
}

#[test]
fn diff_dto_stops_on_the_byte_ceiling_before_the_line_ceiling() {
    let hunks = vec![hunk("@@ a @@", 50, limits::MAX_DIFF_BYTES / 10)];

    let dto = DiffDto::from_hunks("wide.rs", &hunks);

    let bytes: usize = dto.hunks[0]
        .lines
        .iter()
        .flat_map(|line| &line.spans)
        .map(|span| span.t.len())
        .sum();
    assert!(bytes <= limits::MAX_DIFF_BYTES);
    assert!(dto.truncated);
    assert!(dto.hunks[0].lines.len() < 50);
}

#[test]
fn file_dto_caps_content_on_a_character_boundary() {
    let content = "🦀".repeat(limits::MAX_DIFF_BYTES);

    let dto = FileDto::new("big.txt", &content);

    let served: String = dto
        .lines
        .iter()
        .flatten()
        .map(|span| span.t.as_str())
        .collect();
    assert!(dto.truncated);
    assert!(served.len() <= limits::MAX_DIFF_BYTES);
    assert!(content.starts_with(&served));
    assert!(json(&dto).is_object());
}
