//! Replacing the configured tables a hub is spawned with.

use super::super::*;
use crate::test_util::make_repo;

fn startup(command: &str) -> crate::config::StartupCommand {
    crate::config::StartupCommand {
        name: None,
        command: command.to_string(),
        plugin: None,
    }
}

#[test]
fn swapping_the_config_tables_keeps_the_cli_startup_panes() {
    let catalog = Catalog::with_startup_plugins_and_exec(
        crate::config::merge_startup_commands(&[startup("claude")], &["codex".to_string()])
            .unwrap(),
        Vec::new(),
        vec!["codex".to_string()],
    );

    catalog
        .set_config_tables(&[startup("cargo watch")], Vec::new())
        .expect("the merge fits the cap");

    // The file's table was replaced; the --exec pane is not in the file and
    // stays on the end, exactly where a restart would have put it.
    let merged = catalog.startup_commands();
    assert_eq!(merged.len(), 2);
    assert_eq!(merged[0].command, "cargo watch");
    assert_eq!(merged[1].command, "codex");
}

#[test]
fn a_refused_swap_replaces_neither_table() {
    let catalog = Catalog::with_startup_plugins_and_exec(
        vec![startup("claude")],
        Vec::new(),
        // Two --exec panes, so a file table of eight cannot fit the cap of 8.
        vec!["codex".to_string(), "vim".to_string()],
    );
    let plugins = vec![crate::config::PluginConfig {
        name: "recovery".into(),
        command: "nightcrow-recovery".into(),
        ..Default::default()
    }];
    let too_many: Vec<_> = (0..8).map(|i| startup(&format!("echo {i}"))).collect();

    assert!(catalog.set_config_tables(&too_many, plugins).is_err());

    // Neither list moved: a reload does not half-apply.
    assert_eq!(catalog.startup_commands(), vec![startup("claude")]);
    assert!(catalog.plugins().is_empty());
}

#[test]
fn a_repo_opened_after_a_swap_gets_the_new_startup_list() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Catalog::with_startup_and_plugins(vec![startup("claude")], Vec::new());
    catalog.set_paths(std::slice::from_ref(&a));
    let before = Arc::clone(&catalog.entries()[0].terminals);

    catalog
        .set_config_tables(&[startup("cargo watch")], Vec::new())
        .expect("the merge fits the cap");
    catalog.set_paths(&[a.clone(), b.clone()]);

    let entries = catalog.entries();
    // The repository that was already open keeps its hub — and therefore the
    // panes it was opened with. Only the new one starts on the new list.
    assert!(Arc::ptr_eq(&entries[0].terminals, &before));
    assert_eq!(entries[0].terminals.startup_commands(), [startup("claude")]);
    assert_eq!(
        entries[1].terminals.startup_commands(),
        [startup("cargo watch")]
    );
    catalog.shutdown();
    drop((dir_a, dir_b));
}
