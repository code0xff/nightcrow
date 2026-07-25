use super::*;
use crate::test_util::make_repo;

#[test]
fn ids_are_stable_across_catalog_updates() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Catalog::new();

    catalog.set_paths(std::slice::from_ref(&a));
    let first = catalog.list()[0].id.clone();

    // Opening a second repository must not renumber the first.
    catalog.set_paths(&[a.clone(), b.clone()]);
    let after = catalog.list();

    assert_eq!(after[0].id, first);
    assert_ne!(after[1].id, first);
    catalog.shutdown();
    drop((dir_a, dir_b));
}

#[test]
fn a_path_that_leaves_and_returns_keeps_its_id() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Catalog::new();

    catalog.set_paths(&[a.clone(), b.clone()]);
    let a_id = catalog.list()[0].id.clone();
    catalog.set_paths(std::slice::from_ref(&b));
    catalog.set_paths(&[b.clone(), a.clone()]);

    let reopened = catalog.get(&a_id).expect("the id must still resolve");
    assert_eq!(reopened.path, a);
    catalog.shutdown();
    drop((dir_a, dir_b));
}

#[test]
fn add_path_is_idempotent_and_respects_the_cap() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let (dir_c, c) = make_repo();
    let catalog = Catalog::new();

    let id_a = match catalog.add_path(a.clone(), 2) {
        AddOutcome::Added(dto) => dto.id,
        AddOutcome::TooMany => panic!("the first add must succeed"),
    };
    assert_eq!(catalog.len(), 1);

    // Re-adding an open path is a no-op that returns the same identity.
    match catalog.add_path(a.clone(), 2) {
        AddOutcome::Added(dto) => assert_eq!(dto.id, id_a),
        AddOutcome::TooMany => panic!("re-adding an open repo must not be refused"),
    }
    assert_eq!(catalog.len(), 1);

    // A second distinct repo fits under the cap of two.
    assert!(matches!(catalog.add_path(b, 2), AddOutcome::Added(_)));
    assert_eq!(catalog.len(), 2);

    // A third exceeds the cap and is refused without disturbing the set.
    assert!(matches!(catalog.add_path(c, 2), AddOutcome::TooMany));
    assert_eq!(catalog.len(), 2);

    catalog.shutdown();
    drop((dir_a, dir_b, dir_c));
}

#[test]
fn a_browser_added_repo_survives_a_base_update() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Catalog::new();

    catalog.set_paths(std::slice::from_ref(&a));
    let added = match catalog.add_path(b, 10) {
        AddOutcome::Added(dto) => dto.id,
        AddOutcome::TooMany => panic!("the add must succeed"),
    };

    // The TUI opening or closing a tab re-runs set_paths with a new base;
    // a repository opened from the browser must not be dropped by it.
    catalog.set_paths(std::slice::from_ref(&a));
    assert!(
        catalog.get(&added).is_some(),
        "a browser-added repo must survive a base update"
    );

    catalog.shutdown();
    drop((dir_a, dir_b));
}

#[test]
fn remove_path_closes_and_stays_closed_until_reopened() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(&[a.clone(), b.clone()]);
    assert_eq!(catalog.len(), 2);

    catalog.remove_path(&a);
    assert_eq!(catalog.len(), 1);

    // A base re-sync (a TUI tab change) must not resurrect a closed repo.
    catalog.set_paths(&[a.clone(), b.clone()]);
    assert_eq!(catalog.len(), 1, "a closed repo must stay closed");

    // Re-opening it from the browser clears the close and brings it back.
    assert!(matches!(catalog.add_path(a.clone(), 10), AddOutcome::Added(_)));
    assert_eq!(catalog.len(), 2);

    catalog.shutdown();
    drop((dir_a, dir_b));
}

#[test]
fn an_unchanged_path_keeps_its_runtime_and_subscribers() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(std::slice::from_ref(&a));

    let entry = catalog.get(&catalog.list()[0].id).unwrap();
    let _subscription = entry.runtime.subscribe();
    assert_eq!(entry.runtime.subscriber_count(), 1);

    // Adding an unrelated repository must not restart the existing one.
    catalog.set_paths(&[a.clone(), b.clone()]);

    let same = catalog.get(&entry.id).unwrap();
    assert!(
        Arc::ptr_eq(&same.runtime, &entry.runtime),
        "an unchanged path must not get a fresh runtime"
    );
    assert_eq!(
        same.runtime.subscriber_count(),
        1,
        "a catalog update must not drop live SSE clients"
    );
    catalog.shutdown();
    drop((dir_a, dir_b));
}

#[test]
fn removing_a_path_drops_it_from_lookup() {
    let (dir_a, a) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(std::slice::from_ref(&a));
    let id = catalog.list()[0].id.clone();

    catalog.set_paths(&[]);

    assert!(
        catalog.get(&id).is_none(),
        "a closed repo must stop resolving"
    );
    assert!(catalog.is_empty());
    drop(dir_a);
}

#[test]
fn reorder_puts_the_tabs_in_the_requested_order() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let (dir_c, c) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(&[a.clone(), b.clone(), c.clone()]);

    catalog.reorder(&[c.clone(), a.clone(), b.clone()]);

    assert_eq!(catalog.paths(), vec![c, a, b]);
    catalog.shutdown();
    drop((dir_a, dir_b, dir_c));
}

#[test]
fn reorder_drops_unknown_paths_and_appends_omitted_paths() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let (dir_c, c) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(&[a.clone(), b.clone(), c.clone()]);

    catalog.reorder(&[b.clone(), "/no/such/repo".to_string()]);

    assert_eq!(catalog.paths(), vec![b, a, c]);
    catalog.shutdown();
    drop((dir_a, dir_b, dir_c));
}

#[test]
fn a_reordering_survives_a_later_rebuild() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let (dir_c, c) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(&[a.clone(), b.clone()]);
    catalog.reorder(&[b.clone(), a.clone()]);

    catalog.set_paths(&[a.clone(), b.clone()]);
    assert_eq!(catalog.paths(), vec![b.clone(), a.clone()]);
    assert!(matches!(catalog.add_path(c.clone(), 10), AddOutcome::Added(_)));
    assert_eq!(catalog.paths(), vec![b, a, c]);
    catalog.shutdown();
    drop((dir_a, dir_b, dir_c));
}

#[test]
fn reorder_keeps_ids_stable() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(&[a.clone(), b.clone()]);
    let a_id = catalog.list()[0].id.clone();
    let b_id = catalog.list()[1].id.clone();

    catalog.reorder(&[b, a]);

    let after = catalog.list();
    assert_eq!(after[0].id, b_id);
    assert_eq!(after[1].id, a_id);
    catalog.shutdown();
    drop((dir_a, dir_b));
}

#[test]
fn duplicate_paths_collapse_to_one_entry() {
    let (dir_a, a) = make_repo();
    let catalog = Catalog::new();

    catalog.set_paths(&[a.clone(), a.clone(), a.clone()]);

    assert_eq!(catalog.len(), 1, "one worktree is one entry");
    catalog.shutdown();
    drop(dir_a);
}

#[test]
fn an_unknown_id_does_not_resolve() {
    let catalog = Catalog::new();
    assert!(catalog.get("r999").is_none());
    assert!(catalog.get("").is_none());
    assert!(catalog.get("../etc").is_none());
}

#[test]
fn the_dto_exposes_only_the_whitelisted_identity_fields() {
    let (dir_a, a) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(std::slice::from_ref(&a));

    let value = serde_json::to_value(&catalog.list()[0]).unwrap();

    let mut keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, vec!["display_path", "id", "name"]);
    catalog.shutdown();
    drop(dir_a);
}

#[test]
fn display_path_abbreviates_the_home_directory() {
    // A repo under $HOME is sent home-relative, so the payload does not
    // carry the account name. A repo outside it has nothing to abbreviate —
    // the path is the only label that identifies it to the user, and the
    // client is already authenticated to a session that has a shell.
    let home = dirs::home_dir().expect("a home directory");

    assert_eq!(
        display_path(&home.join("code").join("app").to_string_lossy()),
        "~/code/app"
    );
    assert_eq!(display_path(&home.to_string_lossy()), "~");
    assert_eq!(display_path("/opt/elsewhere"), "/opt/elsewhere");
    assert_eq!(
        display_path("/opt/elsewhere/"),
        "/opt/elsewhere",
        "libgit2's trailing separator must not reach the UI"
    );
}

#[test]
fn repo_name_ignores_a_trailing_separator() {
    assert_eq!(repo_name("/code/app/"), "app");
    assert_eq!(repo_name("/code/app"), "app");
}
