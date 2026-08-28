use super::*;

fn paths(membership: &mut CatalogMembership) -> Vec<String> {
    membership
        .members()
        .into_iter()
        .map(|member| member.path)
        .collect()
}

fn id_of(membership: &mut CatalogMembership, path: &str) -> String {
    membership
        .members()
        .into_iter()
        .find(|member| member.path == path)
        .expect("path is served")
        .id
}

#[test]
fn base_and_browser_paths_form_a_stable_deduplicated_union() {
    let mut membership = CatalogMembership::default();
    membership.set_paths(vec!["a".into(), "a".into(), "b".into()]);
    assert!(matches!(
        membership.add_path("c".into(), 3),
        AddMembership::Present(_)
    ));

    membership.set_paths(vec!["b".into(), "a".into()]);

    assert_eq!(paths(&mut membership), ["b", "a", "c"]);
}

#[test]
fn ids_survive_removal_reopen_and_reorder() {
    let mut membership = CatalogMembership::default();
    membership.set_paths(vec!["a".into(), "b".into()]);
    let a_id = id_of(&mut membership, "a");
    let b_id = id_of(&mut membership, "b");

    membership.reorder(&["b".into(), "a".into()]);
    membership.remove_path("a");
    assert!(matches!(
        membership.add_path("a".into(), 2),
        AddMembership::Present(_)
    ));

    assert_eq!(paths(&mut membership), ["b", "a"]);
    assert_eq!(id_of(&mut membership, "a"), a_id);
    assert_eq!(id_of(&mut membership, "b"), b_id);
}

#[test]
fn hidden_paths_stay_closed_across_base_refresh_and_reopen_at_the_end() {
    let mut membership = CatalogMembership::default();
    membership.set_paths(vec!["a".into(), "b".into(), "c".into()]);
    membership.remove_path("b");

    membership.set_paths(vec!["a".into(), "b".into(), "c".into()]);
    assert_eq!(paths(&mut membership), ["a", "c"]);
    assert!(matches!(
        membership.add_path("b".into(), 3),
        AddMembership::Present(_)
    ));
    assert_eq!(paths(&mut membership), ["a", "c", "b"]);
}

#[test]
fn refused_reopen_does_not_clear_hidden_membership() {
    let mut membership = CatalogMembership::default();
    membership.set_paths(vec!["a".into(), "b".into()]);
    membership.remove_path("a");
    membership.set_paths(vec!["a".into(), "b".into()]);

    assert!(matches!(
        membership.add_path("a".into(), 1),
        AddMembership::TooMany
    ));
    membership.set_paths(vec!["a".into(), "b".into()]);

    assert_eq!(paths(&mut membership), ["b"]);
}

#[test]
fn reorder_ignores_unknown_and_duplicate_paths() {
    let mut membership = CatalogMembership::default();
    membership.set_paths(vec!["a".into(), "b".into(), "c".into()]);

    membership.reorder(&["c".into(), "unknown".into(), "c".into()]);

    assert_eq!(paths(&mut membership), ["c", "a", "b"]);
}
