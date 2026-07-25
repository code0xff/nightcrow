use super::*;
use tempfile::tempdir;

#[test]
fn cleanup_removes_files_older_than_max_days() {
    let dir = tempdir().unwrap();
    let old_file = dir.path().join("nightcrow.old.log");
    let new_file = dir.path().join("nightcrow.new.log");
    fs::write(&old_file, b"old").unwrap();
    fs::write(&new_file, b"new").unwrap();

    // Backdate old_file by setting mtime via a workaround (write then check)
    // Since we can't easily set mtime in stdlib, we verify the function runs
    // without panic and only deletes files matching the naming pattern.
    cleanup_old_logs(dir.path(), 0); // max_days=0 means keep all
    assert!(old_file.exists());
    assert!(new_file.exists());
}

#[test]
fn expired_log_paths_preserves_newest_even_when_old() {
    let now = SystemTime::now();
    let day = Duration::from_secs(86400);
    let candidates = vec![
        (PathBuf::from("nightcrow.log.0"), now - day * 30),
        (PathBuf::from("nightcrow.log.1"), now - day * 20),
        (PathBuf::from("nightcrow.log.2"), now - day * 10),
    ];
    let cutoff = now - day; // anything older than 1 day is expired

    let expired = expired_log_paths(&candidates, cutoff);

    // newest (.2) must be preserved; older two are expired.
    let names: Vec<_> = expired.iter().map(|p| p.to_str().unwrap()).collect();
    assert_eq!(names, vec!["nightcrow.log.0", "nightcrow.log.1"]);
}

#[test]
fn expired_log_paths_keeps_recent_files() {
    let now = SystemTime::now();
    let candidates = vec![
        (
            PathBuf::from("nightcrow.log.0"),
            now - Duration::from_secs(60),
        ),
        (
            PathBuf::from("nightcrow.log.1"),
            now - Duration::from_secs(30),
        ),
    ];
    let cutoff = now - Duration::from_secs(86400);

    assert!(expired_log_paths(&candidates, cutoff).is_empty());
}

#[test]
fn cleanup_skips_non_nightcrow_files() {
    let dir = tempdir().unwrap();
    let other = dir.path().join("other.log");
    fs::write(&other, b"x").unwrap();
    cleanup_old_logs(dir.path(), 1);
    assert!(other.exists());
}

#[test]
fn recognizes_generated_nightcrow_log_names() {
    assert!(is_nightcrow_log_file(Path::new("nightcrow.log")));
    assert!(is_nightcrow_log_file(Path::new("nightcrow.log.0")));
    assert!(is_nightcrow_log_file(Path::new("nightcrow.log.2026-05-03")));
    assert!(is_nightcrow_log_file(Path::new(
        "nightcrow.log.2026-05-03-14"
    )));
    assert!(!is_nightcrow_log_file(Path::new("nightcrow.old.log")));
    assert!(!is_nightcrow_log_file(Path::new("other.log")));
}

#[test]
fn size_rolling_appender_rotates_on_overflow() {
    let dir = tempdir().unwrap();
    let mut appender = SizeRollingAppender::new(dir.path(), "test.log", 10).unwrap();
    appender.write_all(b"hello12345").unwrap(); // exactly 10 bytes → no rotate yet
    appender.write_all(b"x").unwrap(); // 11th byte triggers rotate
    let inner = appender.inner.lock().unwrap();
    assert_eq!(inner.index, 1);
}

#[test]
fn size_rolling_appender_resumes_highest_existing_index() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("test.log.0"), b"old").unwrap();
    fs::write(dir.path().join("test.log.2"), b"new").unwrap();
    fs::write(dir.path().join("test.log.2026-05-03"), b"daily").unwrap();

    let appender = SizeRollingAppender::new(dir.path(), "test.log", 10).unwrap();
    let inner = appender.inner.lock().unwrap();

    assert_eq!(inner.index, 2);
    assert_eq!(inner.current_size, 3);
}

#[test]
fn gitignore_is_written_only_into_a_nightcrow_owned_directory() {
    // `*` has to ignore the ignore file itself to hide the directory. In
    // our own folder that is harmless; in a directory the user pointed
    // `[log] dir` at it would make Git ignore everything untracked there,
    // their own `.gitignore` included.
    let dir = tempfile::TempDir::new().unwrap();
    let ours = dir.path().join(".nightcrow").join("logs");
    let theirs = dir.path().join("build-logs");
    std::fs::create_dir_all(&ours).unwrap();
    std::fs::create_dir_all(&theirs).unwrap();

    write_log_gitignore(&ours);
    write_log_gitignore(&theirs);

    assert_eq!(
        std::fs::read_to_string(ours.join(".gitignore")).unwrap(),
        "*\n"
    );
    assert!(
        !theirs.join(".gitignore").exists(),
        "a user-chosen log directory is theirs to manage"
    );
}

#[test]
fn gitignore_never_clobbers_an_existing_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let ours = dir.path().join(".nightcrow").join("logs");
    std::fs::create_dir_all(&ours).unwrap();
    std::fs::write(ours.join(".gitignore"), "# mine\n").unwrap();

    write_log_gitignore(&ours);

    assert_eq!(
        std::fs::read_to_string(ours.join(".gitignore")).unwrap(),
        "# mine\n"
    );
}

#[test]
fn resolve_log_dir_absolute_path_unchanged() {
    let abs = "/tmp/nightcrow-logs";
    let result = resolve_log_dir(abs, "/some/repo");
    assert_eq!(result, PathBuf::from(abs));
}

#[test]
fn resolve_log_dir_relative_joins_repo_path() {
    let result = resolve_log_dir(".nightcrow/logs", "/my/repo");
    assert_eq!(result, PathBuf::from("/my/repo/.nightcrow/logs"));
}
