use super::*;
use tempfile::TempDir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn executable_fixture(path: &Path) {
    std::fs::write(path, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(windows)]
fn executable_fixture(path: &Path) {
    std::fs::write(path, b"@echo off\r\nexit /b 0\r\n").unwrap();
}

/// Source executable plus the plugins directory it installs into, both inside
/// one temp dir that is removed when the returned handle drops.
fn workspace() -> (TempDir, PathBuf, PathBuf) {
    let root = TempDir::new().expect("a temp dir");
    let source = root.path().join("watcher");
    executable_fixture(&source);
    let base = root.path().join("plugins");
    (root, source, base)
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn installing_copies_the_file_sets_owner_only_mode_and_reports_created() {
    let (_root, source, base) = workspace();

    let outcome = install(&base, &source, Some("watcher"), false).expect("install to succeed");

    let InstallOutcome::Created(path) = outcome else {
        panic!("expected Created, got {outcome:?}");
    };
    assert_eq!(path, base.join("watcher"));
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&source).unwrap()
    );
    #[cfg(unix)]
    assert_eq!(mode_of(&path), 0o700);
}

#[test]
fn installing_over_an_existing_name_without_force_is_refused_and_keeps_the_original_bytes() {
    let (_root, source, base) = workspace();
    install(&base, &source, Some("watcher"), false).unwrap();
    let other = source.parent().unwrap().join("other");
    executable_fixture(&other);

    let outcome = install(&base, &other, Some("watcher"), false).expect("no error, a report");

    let InstallOutcome::AlreadyExists(path) = outcome else {
        panic!("expected AlreadyExists, got {outcome:?}");
    };
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&source).unwrap()
    );
}

#[test]
fn installing_with_force_replaces_the_installed_plugin() {
    let (_root, source, base) = workspace();
    install(&base, &source, Some("watcher"), false).unwrap();
    let other = source.parent().unwrap().join("other");
    executable_fixture(&other);

    let outcome = install(&base, &other, Some("watcher"), true).expect("install to succeed");

    let InstallOutcome::Replaced(path) = outcome else {
        panic!("expected Replaced, got {outcome:?}");
    };
    assert_eq!(
        std::fs::read(&path).unwrap(),
        std::fs::read(&other).unwrap()
    );
    #[cfg(unix)]
    assert_eq!(mode_of(&path), 0o700);
}

#[test]
fn a_name_that_is_not_a_safe_single_filename_is_rejected() {
    let unsafe_names = [
        "../escape",
        "..",
        ".",
        "sub/dir",
        "back\\slash",
        "-rf",
        "has space",
        "semi;colon",
        "quote\"d",
        "null\0byte",
        "",
    ];

    for name in unsafe_names {
        assert!(
            validate_name(name).is_err(),
            "expected {name:?} to be rejected"
        );
    }
}

#[test]
fn an_unsafe_name_is_rejected_by_install_and_remove_before_any_filesystem_write() {
    let (_root, source, base) = workspace();

    assert!(install(&base, &source, Some("../escape"), false).is_err());
    assert!(remove(&base, "../escape").is_err());
    assert!(!base.exists(), "nothing should have been created");
}

#[test]
fn a_source_that_does_not_exist_is_rejected() {
    let (_root, source, base) = workspace();
    let missing = source.parent().unwrap().join("nope");

    let err = install(&base, &missing, None, false).expect_err("a missing source is an error");

    assert!(
        format!("{err:#}").contains("cannot be read"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn a_source_that_is_a_directory_is_rejected() {
    let (_root, source, base) = workspace();
    let dir = source.parent().unwrap().join("adir");
    std::fs::create_dir(&dir).unwrap();

    let err = install(&base, &dir, None, false).expect_err("a directory source is an error");

    assert!(
        format!("{err:#}").contains("not a regular file"),
        "unexpected error: {err:#}"
    );
}

#[cfg(unix)]
#[test]
fn a_source_that_is_not_executable_is_rejected() {
    let (_root, source, base) = workspace();
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).unwrap();

    let err =
        install(&base, &source, None, false).expect_err("a non-executable source is an error");

    assert!(
        format!("{err:#}").contains("not executable"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn listing_an_empty_directory_yields_nothing_and_does_not_error() {
    let (_root, _source, base) = workspace();
    std::fs::create_dir_all(&base).unwrap();

    assert_eq!(
        list(&base).expect("an empty list, not an error"),
        Vec::<String>::new()
    );
}

#[test]
fn listing_a_directory_that_does_not_exist_yet_yields_nothing() {
    let (_root, _source, base) = workspace();

    assert_eq!(
        list(&base).expect("an empty list, not an error"),
        Vec::<String>::new()
    );
}

#[test]
fn listing_reports_installed_names_in_sorted_order() {
    let (_root, source, base) = workspace();
    install(&base, &source, Some("zulu"), false).unwrap();
    install(&base, &source, Some("alpha"), false).unwrap();
    std::fs::create_dir(base.join("subdir")).unwrap();

    assert_eq!(list(&base).unwrap(), vec!["alpha", "zulu"]);
}

#[test]
fn removing_an_installed_plugin_deletes_the_file_and_reports_removed() {
    let (_root, source, base) = workspace();
    install(&base, &source, Some("watcher"), false).unwrap();

    let outcome = remove(&base, "watcher").expect("remove to succeed");

    let RemoveOutcome::Removed(path) = outcome else {
        panic!("expected Removed, got {outcome:?}");
    };
    assert!(!path.exists());
}

#[test]
fn removing_a_name_that_is_not_installed_reports_not_installed() {
    let (_root, _source, base) = workspace();
    std::fs::create_dir_all(&base).unwrap();

    let outcome = remove(&base, "watcher").expect("no error, a report");

    let RemoveOutcome::NotInstalled(name) = outcome else {
        panic!("expected NotInstalled, got {outcome:?}");
    };
    assert_eq!(name, "watcher");
}

#[test]
fn the_default_name_is_derived_from_the_source_file_stem() {
    let (_root, source, base) = workspace();
    let stemmed = source.parent().unwrap().join("my-plugin.sh");
    std::fs::copy(&source, &stemmed).unwrap();
    executable_fixture(&stemmed);

    install(&base, &stemmed, None, false).expect("install to succeed");

    assert_eq!(list(&base).unwrap(), vec!["my-plugin"]);
}

#[test]
fn status_reports_declaration_enablement_and_the_number_of_opted_in_panes() {
    let mut cfg = crate::config::Config::default();
    cfg.plugins.push(crate::config::PluginConfig {
        name: "watcher".into(),
        command: "watcher".into(),
        enabled: true,
        ..Default::default()
    });
    for _ in 0..2 {
        cfg.startup_commands.push(crate::config::StartupCommand {
            name: None,
            command: "bash".into(),
            plugin: Some("watcher".into()),
        });
    }

    assert_eq!(
        status(&cfg, "watcher"),
        PluginStatus {
            declared: true,
            enabled: true,
            opt_ins: 2,
        }
    );
    assert_eq!(
        status(&cfg, "absent"),
        PluginStatus {
            declared: false,
            enabled: false,
            opt_ins: 0,
        }
    );
}

#[test]
fn the_printed_config_snippet_leaves_the_plugin_off_and_grants_no_resume_flags() {
    let snippet = config_snippet("watcher", Path::new("/home/u/.nightcrow/plugins/watcher"));

    assert!(snippet.contains("name = \"watcher\""));
    assert!(snippet.contains("command = \"/home/u/.nightcrow/plugins/watcher\""));
    assert!(snippet.contains("allowed_resume_flags = []"));
    assert!(snippet.contains("enabled = false"));
}
