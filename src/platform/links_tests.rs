use super::{LinkTarget, parse_target};
use std::path::PathBuf;

#[test]
fn parses_web_links_without_rewriting_the_target() {
    assert_eq!(
        parse_target("https://example.test/a path")
            .unwrap_err()
            .to_string(),
        "web link contains whitespace or a backslash"
    );
    assert_eq!(
        parse_target("https://example.test/a%20path#L4").unwrap(),
        LinkTarget::Web("https://example.test/a%20path#L4".into())
    );
}

#[test]
fn parses_relative_paths_with_line_and_column() {
    assert!(parse_target("한글.txt").is_ok());
    assert!(parse_target("docs/readme.md#L9-").is_err());
    assert_eq!(
        parse_target("src/한 파일.rs:12:7").unwrap(),
        LinkTarget::File {
            path: PathBuf::from("src/한 파일.rs"),
            line: Some(12),
            column: Some(7),
        }
    );
    assert_eq!(
        parse_target("docs/readme.md#L9C3-L10C4").unwrap(),
        LinkTarget::File {
            path: PathBuf::from("docs/readme.md"),
            line: Some(9),
            column: Some(3),
        }
    );
    assert_eq!(
        parse_target("src/file-2026.rs:12").unwrap(),
        LinkTarget::File {
            path: PathBuf::from("src/file-2026.rs"),
            line: Some(12),
            column: None,
        }
    );
}

#[test]
fn rejects_unsafe_or_ambiguous_targets_before_launching() {
    for target in [
        "javascript:alert(1)",
        "file://server/share/readme.md",
        "file:",
        "file:😀",
        "file:한글",
        "file:aa/C:/temp/a.txt",
        r"\\?\C:\secret.txt",
        "../secret.txt",
        "src/file.rs\n:3",
        "src/file.rs:0",
        "https://example.test/%0A",
        "file:///tmp/readme.md#section",
    ] {
        assert!(
            parse_target(target).is_err(),
            "accepted unsafe target {target:?}"
        );
    }
}

#[cfg(windows)]
#[test]
fn parses_windows_drive_paths_and_file_urls() {
    assert_eq!(
        parse_target(r"C:\work\space file.rs:4:2").unwrap(),
        LinkTarget::File {
            path: PathBuf::from(r"C:\work\space file.rs"),
            line: Some(4),
            column: Some(2),
        }
    );
    assert_eq!(
        parse_target("file:///C:/work/space%20file.rs#L4").unwrap(),
        LinkTarget::File {
            path: PathBuf::from(r"C:\work\space file.rs"),
            line: Some(4),
            column: None,
        }
    );
    assert!(matches!(
        parse_target("FILE:///C:/work/space%20file.rs"),
        Ok(LinkTarget::File { .. })
    ));
    assert!(parse_target(r"C:\work\space.txt:secret").is_err());
}

#[cfg(unix)]
#[test]
fn parses_absolute_file_urls_but_plain_paths_stay_repo_relative() {
    assert_eq!(
        parse_target("file:///tmp/space%20file.md:6").unwrap(),
        LinkTarget::File {
            path: PathBuf::from("/tmp/space file.md"),
            line: Some(6),
            column: None,
        }
    );
    assert!(parse_target("/tmp/space file.md").is_err());
}
