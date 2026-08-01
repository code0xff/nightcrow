//! Tab order: what a reorder request is reconciled against, and what survives a
//! rebuild.

use super::served;
use super::*;

#[test]
fn reorder_puts_the_tabs_in_the_requested_order() {
    let (dir_a, a) = make_repo();
    let (dir_b, b) = make_repo();
    let (dir_c, c) = make_repo();
    let catalog = Catalog::new();
    catalog.set_paths(&[a.clone(), b.clone(), c.clone()]);

    catalog.reorder(&[c.clone(), a.clone(), b.clone()]);

    assert_eq!(catalog.paths(), vec![served(&c), served(&a), served(&b)]);
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

    assert_eq!(catalog.paths(), vec![served(&b), served(&a), served(&c)]);
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
    assert_eq!(catalog.paths(), vec![served(&b), served(&a)]);
    assert!(matches!(
        catalog.add_path(c.clone(), 10),
        AddOutcome::Added(_)
    ));
    assert_eq!(catalog.paths(), vec![served(&b), served(&a), served(&c)]);
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
