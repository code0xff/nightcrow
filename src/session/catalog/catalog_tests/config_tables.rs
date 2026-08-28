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

fn plugin(name: &str) -> crate::config::PluginConfig {
    crate::config::PluginConfig {
        name: name.to_string(),
        command: "nightcrow-recovery".to_string(),
        ..Default::default()
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
fn a_swap_hands_back_the_repositories_the_caller_must_tell() {
    let (dir, path) = make_repo();
    let catalog = Catalog::with_startup_and_plugins(vec![startup("claude")], Vec::new());
    catalog.set_paths(std::slice::from_ref(&path));

    let told = catalog
        .set_config_tables(&[startup("cargo watch")], vec![plugin("recovery")])
        .expect("the merge fits the cap");

    // The fan-out list is the served set as of the swap itself, not one fetched
    // afterwards: that is what stops a repository opened in the same beat from
    // falling between the two and running the previous plugin table for good.
    assert_eq!(told.len(), 1);
    assert!(Arc::ptr_eq(&told[0], &catalog.entries()[0]));
    catalog.shutdown();
    drop(dir);
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

#[test]
fn concurrent_open_and_config_swap_cannot_miss_each_other() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Arc::new(Catalog::with_startup_and_plugins(
        vec![startup("old")],
        Vec::new(),
    ));
    catalog.set_paths(std::slice::from_ref(&a));
    let barrier = Arc::new(std::sync::Barrier::new(3));

    let opening = {
        let catalog = Arc::clone(&catalog);
        let barrier = Arc::clone(&barrier);
        let a = a.clone();
        let b = b.clone();
        std::thread::spawn(move || {
            barrier.wait();
            catalog.set_paths(&[a, b]);
        })
    };
    let swapping = {
        let catalog = Arc::clone(&catalog);
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            barrier.wait();
            catalog
                .set_config_tables(&[startup("new")], Vec::new())
                .expect("the merge fits the cap")
        })
    };
    barrier.wait();
    opening.join().unwrap();
    let told = swapping.join().unwrap();

    let b = crate::git::resolve_repo_path(std::path::Path::new(&b))
        .to_string_lossy()
        .into_owned();
    let opened = catalog
        .entries()
        .into_iter()
        .find(|entry| entry.path == b)
        .expect("the concurrent open committed");
    let was_in_swap = told.iter().any(|entry| Arc::ptr_eq(entry, &opened));
    let spawned_from_new_table = opened.terminals.startup_commands() == [startup("new")];
    assert!(
        was_in_swap || spawned_from_new_table,
        "an opened repository must be told by the swap or spawn from its tables"
    );
    catalog.shutdown();
    drop((dir_a, dir_b));
}
