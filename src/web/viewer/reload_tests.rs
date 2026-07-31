//! Re-reading the config file into a live session.

use super::*;
use crate::test_util::{make_repo, session_state};

/// A config file with the given body, in its own directory.
fn config_file(dir: &std::path::Path, body: &str) -> std::path::PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(&path, body).expect("write the config file");
    path
}

const ONE_PANE: &str = r#"
[[startup_command]]
name = "Claude"
command = "claude"
"#;

#[test]
fn a_reload_reports_what_the_file_now_declares() {
    let dir = tempfile::TempDir::new().unwrap();
    let (repo_dir, repo) = make_repo();
    let state = session_state(std::slice::from_ref(&repo), dir.path());
    let path = config_file(dir.path(), ONE_PANE);

    let report = reload_config_at(&state, &path).expect("a valid file must apply");

    assert_eq!(report.startup_commands, 1);
    assert_eq!(report.plugins, 0);
    assert_eq!(report.repos, 1, "the one open repository was re-applied");
    state.catalog.shutdown();
    drop((repo_dir, dir));
}

#[test]
fn a_reload_replaces_the_startup_list_for_repos_opened_afterwards() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = session_state(&[], dir.path());
    let path = config_file(dir.path(), ONE_PANE);

    reload_config_at(&state, &path).expect("a valid file must apply");

    let startup = state.catalog.startup_commands();
    assert_eq!(startup.len(), 1);
    assert_eq!(startup[0].command, "claude");
    assert_eq!(startup[0].name.as_deref(), Some("Claude"));
    state.catalog.shutdown();
    drop(dir);
}

#[test]
fn a_file_that_does_not_parse_changes_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = session_state(&[], dir.path());
    reload_config_at(&state, &config_file(dir.path(), ONE_PANE)).expect("the first file applies");

    let broken = config_file(dir.path(), "[[startup_command]]\nname = \"unclosed");
    let err = reload_config_at(&state, &broken).expect_err("a file that does not parse must fail");

    assert!(
        err.to_string().contains("parsing config file"),
        "the error must name what went wrong: {err}"
    );
    // Still what the last good file said.
    assert_eq!(state.catalog.startup_commands().len(), 1);
    state.catalog.shutdown();
    drop(dir);
}

#[test]
fn a_file_that_fails_validation_changes_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = session_state(&[], dir.path());
    reload_config_at(&state, &config_file(dir.path(), ONE_PANE)).expect("the first file applies");

    // Parses, but an empty command is refused — the same check startup makes.
    let invalid = config_file(dir.path(), "[[startup_command]]\ncommand = \"  \"\n");
    let err = reload_config_at(&state, &invalid).expect_err("validation must refuse this");

    assert!(
        err.to_string().contains("must not be empty"),
        "the error must name the offending key: {err}"
    );
    assert_eq!(state.catalog.startup_commands().len(), 1);
    state.catalog.shutdown();
    drop(dir);
}

#[test]
fn a_missing_file_is_refused_rather_than_read_as_nothing_configured() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = session_state(&[], dir.path());
    reload_config_at(&state, &config_file(dir.path(), ONE_PANE)).expect("the first file applies");

    // Deleting the file and reloading must not be a way to silently stop every
    // configured plugin.
    let path = dir.path().join("config.toml");
    std::fs::remove_file(&path).expect("remove the config file");
    assert!(reload_config_at(&state, &path).is_err());
    assert_eq!(state.catalog.startup_commands().len(), 1);
    state.catalog.shutdown();
    drop(dir);
}

#[test]
fn a_reload_refused_by_the_pane_cap_replaces_neither_table() {
    let dir = tempfile::TempDir::new().unwrap();
    let state = session_state(&[], dir.path());
    // The cap is on the file's own table here, so validation catches it before
    // the catalog is asked. Either way the session must be untouched.
    let body: String = (0..crate::config::MAX_STARTUP_COMMANDS + 1)
        .map(|i| format!("[[startup_command]]\ncommand = \"echo {i}\"\n"))
        .collect();
    let err = reload_config_at(&state, &config_file(dir.path(), &body))
        .expect_err("too many panes must be refused");

    assert!(
        err.to_string().contains("at most"),
        "the error must say what the limit is: {err}"
    );
    assert!(state.catalog.startup_commands().is_empty());
    state.catalog.shutdown();
    drop(dir);
}

/// The summary is shown to a person in two places, so its grammar is part of the
/// contract. A live run read back "1 startup pane apply", which is what sent this
/// case here.
#[test]
fn the_summary_agrees_with_its_own_counts() {
    let one = ReloadReport {
        plugins: 1,
        startup_commands: 1,
        repos: 1,
        unreachable: 0,
    };
    assert_eq!(
        one.summary(),
        "config reloaded: 1 plugin across 1 open project; \
         1 startup pane applies to newly opened projects"
    );
    let many = ReloadReport {
        plugins: 2,
        startup_commands: 3,
        repos: 4,
        unreachable: 0,
    };
    assert_eq!(
        many.summary(),
        "config reloaded: 2 plugins across 4 open projects; \
         3 startup panes apply to newly opened projects"
    );
    let none = ReloadReport {
        plugins: 0,
        startup_commands: 0,
        repos: 0,
        unreachable: 0,
    };
    assert_eq!(
        none.summary(),
        "config reloaded: 0 plugins across 0 open projects; no startup panes configured"
    );
}

/// A repository that could not be told keeps its plugin children, so the sentence
/// must not read as if every open project had been re-applied.
#[test]
fn the_summary_owns_up_to_the_repositories_it_could_not_tell() {
    let one = ReloadReport {
        plugins: 1,
        startup_commands: 1,
        repos: 2,
        unreachable: 1,
    };
    assert_eq!(
        one.summary(),
        "config reloaded: 1 plugin across 2 open projects (1 was too busy to be told); \
         1 startup pane applies to newly opened projects"
    );
    let many = ReloadReport {
        plugins: 1,
        startup_commands: 1,
        repos: 0,
        unreachable: 3,
    };
    assert_eq!(
        many.summary(),
        "config reloaded: 1 plugin across 0 open projects (3 were too busy to be told); \
         1 startup pane applies to newly opened projects"
    );
}
