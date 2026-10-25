use super::*;
use tempfile::TempDir;

#[test]
fn an_existing_schema_v1_file_remains_readable_after_write_and_reopen() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("p.db");
    // This file was created by rusqlite 0.40.2, before the dependency upgrade.
    std::fs::write(
        &path,
        include_bytes!("../tests/fixtures/legacy_schema_v1.db"),
    )
    .unwrap();

    let mut store = Store::open(&path).unwrap();
    assert_eq!(store.search("legacy sqlite", 10).unwrap().len(), 1);
    assert_eq!(store.recent(10).unwrap()[0].author, "old-helper");
    let journal_mode: String = store
        .conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal_mode, "wal");

    store
        .write_note("freshnote marker", &[], "new-helper", 200)
        .unwrap();
    drop(store);

    let store = Store::open(&path).unwrap();
    let notes = store.recent(10).unwrap();
    assert_eq!(notes.len(), 2);
    assert_eq!(store.search("legacy", 10).unwrap().len(), 1);
    assert_eq!(store.search("freshnote", 10).unwrap().len(), 1);
    assert_eq!(notes[0].author, "new-helper");
    assert_eq!(notes[1].author, "old-helper");

    let version: i64 = store
        .conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, SCHEMA_VERSION);
}
