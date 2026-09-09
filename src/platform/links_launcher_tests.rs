use super::{Editor, editor_args, spawn_editor};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

#[cfg(not(windows))]
#[test]
fn gedit_uses_plus_line_and_column_arguments() {
    let args = editor_args(
        Editor::Gedit,
        Path::new("src/main.rs"),
        Some(12),
        Some(7),
        None,
    );
    assert_eq!(
        args,
        vec![OsString::from("+12:7"), OsString::from("src/main.rs")]
    );
}

#[test]
fn code_uses_one_goto_location_argument() {
    let args = editor_args(
        Editor::Code,
        Path::new("src/main.rs"),
        Some(12),
        Some(7),
        Some("src/main.rs:12:7"),
    );
    assert_eq!(
        args,
        vec![OsString::from("--goto"), OsString::from("src/main.rs:12:7")]
    );
}

#[test]
fn immediate_editor_failure_is_reported_without_running_a_shell() {
    if std::env::var_os("NIGHTCROW_LINK_EDITOR_FIXTURE").is_some() {
        std::process::exit(97);
    }
    let executable = std::env::current_exe().expect("test executable");
    let mut command = Command::new(executable);
    command
        .args([
            "--exact",
            "platform::links::tests::immediate_editor_failure_is_reported_without_running_a_shell",
            "--nocapture",
        ])
        .env("NIGHTCROW_LINK_EDITOR_FIXTURE", "1");
    let error = spawn_editor(&mut command, Path::new("fixture"))
        .expect_err("the guarded fixture must fail immediately");
    assert!(error.to_string().contains("exited with"));
}
