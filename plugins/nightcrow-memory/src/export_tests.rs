use super::*;
use tempfile::TempDir;

#[test]
fn export_lists_the_board_and_every_note_oldest_first() {
    let dir = TempDir::new().unwrap();
    let mut store = Store::open(&dir.path().join("p.db")).unwrap();
    store
        .write_note("first fact", &["a".to_string()], "x@1", 10)
        .unwrap();
    store.write_note("second fact", &[], "y@2", 20).unwrap();
    store.post_board("on\nthe parser", "x@1", 30, 100).unwrap();

    let mut out = Vec::new();
    write(&store, 40, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();

    assert!(
        text.contains("- **x@1** (just now): on the parser"),
        "{text}"
    );
    assert!(text.contains("## Notes (2)"));
    assert!(
        text.contains("### #1 · x@1 · just now · a\n\nfirst fact"),
        "{text}"
    );
    assert!(text.find("first fact").unwrap() < text.find("second fact").unwrap());
}

#[test]
fn an_empty_store_exports_its_headings() {
    let dir = TempDir::new().unwrap();
    let store = Store::open(&dir.path().join("p.db")).unwrap();
    let mut out = Vec::new();
    write(&store, 0, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("_Nothing posted._"));
    assert!(text.contains("## Notes (0)"));
}
