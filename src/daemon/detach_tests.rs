use super::{background_command, child_args, is_detached_child};
use std::ffi::OsString;

fn args(list: &[&str]) -> Vec<OsString> {
    list.iter().map(OsString::from).collect()
}

/// Nothing here spawns the binary under test. Re-exec'ing `current_exe` from a
/// test starts the whole suite again as a background child, which is not a
/// hypothetical: it happened, and 46 tests failed while dozens of copies of the
/// harness fought over the same machine. The spawn itself is covered by running
/// the real thing, not from in here.
#[test]
fn a_process_with_no_marker_is_the_foreground_copy() {
    assert!(!is_detached_child());
}

#[test]
fn the_background_copy_is_not_told_to_detach_again() {
    // Otherwise it would spawn another, which would spawn another.
    let filtered = child_args(args(&["--port", "9000", "-d", "--exec", "claude"]).into_iter());

    assert_eq!(filtered, args(&["--port", "9000", "--exec", "claude"]));
}

#[test]
fn the_long_form_of_the_flag_is_dropped_too() {
    let filtered = child_args(args(&["--detach", "--bind", "127.0.0.1"]).into_iter());

    assert_eq!(filtered, args(&["--bind", "127.0.0.1"]));
}

#[test]
fn the_attach_subcommand_is_dropped_so_the_child_runs_the_daemon() {
    // `nightcrow -d attach` starts the session here and attaches from the
    // foreground copy; a child that inherited `attach` would attach to the
    // daemon that does not exist yet instead of being it.
    let filtered = child_args(args(&["-d", "attach"]).into_iter());

    assert!(filtered.is_empty(), "{filtered:?}");
}

#[test]
fn every_other_argument_is_passed_through_in_order() {
    let given = args(&["--exec", "a", "--exec", "b", "--port", "1"]);

    assert_eq!(child_args(given.clone().into_iter()), given);
}

#[test]
fn the_log_directory_is_created_before_the_daemon_needs_it() {
    // A first run has no ~/.nightcrow yet, and a background process with no
    // terminal has nowhere to report that it could not start.
    let dir = tempfile::TempDir::new().unwrap();
    let log = dir.path().join("nested").join("daemon.out");

    background_command(std::path::Path::new("/bin/true"), &[], &log).expect("builds");

    assert!(log.parent().unwrap().is_dir());
    assert!(log.exists(), "the log file is opened up front");
}

#[test]
fn an_unusable_log_path_is_reported_rather_than_swallowed() {
    // The foreground copy is about to exit, so this is the user's only chance
    // to learn the daemon never started.
    let dir = tempfile::TempDir::new().unwrap();
    // A file where a directory has to be — create_dir_all fails on it.
    let blocker = dir.path().join("blocked");
    std::fs::write(&blocker, b"not a directory").unwrap();

    let err = background_command(
        std::path::Path::new("/bin/true"),
        &[],
        &blocker.join("daemon.out"),
    )
    .expect_err("an unusable log path must be reported");

    assert!(err.to_string().contains("blocked"), "{err:#}");
}

#[test]
fn existing_output_is_appended_to_rather_than_truncated() {
    // Restarting a background session must not erase why the last one stopped.
    let dir = tempfile::TempDir::new().unwrap();
    let log = dir.path().join("daemon.out");
    std::fs::write(&log, b"earlier run\n").unwrap();

    background_command(std::path::Path::new("/bin/true"), &[], &log).expect("builds");

    assert_eq!(std::fs::read_to_string(&log).unwrap(), "earlier run\n");
}
