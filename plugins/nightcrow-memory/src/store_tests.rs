use super::*;
use tempfile::TempDir;

fn open() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let store = Store::open(&dir.path().join("memory").join("p.db")).unwrap();
    (dir, store)
}

fn tags(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

#[test]
fn a_written_note_is_read_back_with_its_author_and_tags() {
    let (_dir, mut store) = open();
    let id = store
        .write_note(
            "  uses npm, not pnpm  ",
            &tags(&["Build", "npm"]),
            "a@1",
            100,
        )
        .unwrap();
    let note = store.note_by_id(id).unwrap().unwrap();
    assert_eq!(note.body, "uses npm, not pnpm");
    assert_eq!(note.tags, ["build", "npm"]);
    assert_eq!(note.author, "a@1");
    assert_eq!(note.created_at, 100);
    assert_eq!(note.expires_at, None);
}

#[test]
fn search_finds_a_note_by_any_word_and_ranks_the_closer_one_first() {
    let (_dir, mut store) = open();
    store
        .write_note("the viewer bundle is committed", &[], "a", 1)
        .unwrap();
    let both = store
        .write_note("rebuild the viewer bundle with npm", &[], "a", 2)
        .unwrap();
    store
        .write_note("rustfmt runs on stable in CI", &[], "a", 3)
        .unwrap();

    let found = store.search("npm bundle", 10).unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id, both);
}

#[test]
fn search_matches_a_word_with_an_ending_attached() {
    let (_dir, mut store) = open();
    store.write_note("메모리를 공유한다", &[], "a", 1).unwrap();
    assert_eq!(store.search("메모리", 10).unwrap().len(), 1);
}

#[test]
fn search_finds_a_note_by_its_tag() {
    let (_dir, mut store) = open();
    store
        .write_note("body without the word", &tags(&["release"]), "a", 1)
        .unwrap();
    assert_eq!(store.search("release", 10).unwrap().len(), 1);
}

#[test]
fn query_punctuation_is_text_not_fts_syntax() {
    let (_dir, mut store) = open();
    store.write_note("cargo fmt --check", &[], "a", 1).unwrap();
    for query in ["--check", "fmt: \"check\"", "NOT (cargo", "check*", "a:b"] {
        store
            .search(query, 10)
            .unwrap_or_else(|e| panic!("{query:?}: {e}"));
    }
}

#[test]
fn a_query_with_no_words_is_refused() {
    let (_dir, store) = open();
    for query in ["", "   ", "--- :: \"\""] {
        assert!(store.search(query, 10).is_err(), "{query:?}");
    }
}

#[test]
fn recent_is_newest_first_and_honours_the_limit() {
    let (_dir, mut store) = open();
    for at in 1..=5 {
        store
            .write_note(&format!("note {at}"), &[], "a", at)
            .unwrap();
    }
    let recent = store.recent(2).unwrap();
    assert_eq!(
        recent.iter().map(|e| e.created_at).collect::<Vec<_>>(),
        [5, 4]
    );
}

#[test]
fn a_read_never_returns_more_than_the_cap() {
    let (_dir, mut store) = open();
    for at in 0..(MAX_RESULTS as i64 + 5) {
        store.write_note("same words here", &[], "a", at).unwrap();
    }
    assert_eq!(store.recent(usize::MAX).unwrap().len(), MAX_RESULTS);
    assert_eq!(store.search("same", usize::MAX).unwrap().len(), MAX_RESULTS);
    assert_eq!(store.recent(0).unwrap().len(), 1);
}

#[test]
fn a_deleted_note_is_gone_from_reads_and_from_search() {
    let (_dir, mut store) = open();
    let id = store.write_note("obsolete fact", &[], "a", 1).unwrap();
    assert!(store.delete_note(id).unwrap());
    assert!(store.recent(10).unwrap().is_empty());
    assert!(store.search("obsolete", 10).unwrap().is_empty());
    assert!(!store.delete_note(id).unwrap());
}

#[test]
fn empty_and_oversized_text_is_refused_not_truncated() {
    let (_dir, mut store) = open();
    assert!(store.write_note("   ", &[], "a", 1).is_err());
    let at_cap = "x".repeat(MAX_BODY_BYTES);
    store.write_note(&at_cap, &[], "a", 1).unwrap();
    let over = "x".repeat(MAX_BODY_BYTES + 1);
    let error = store
        .write_note(&over, &[], "a", 1)
        .unwrap_err()
        .to_string();
    assert!(error.contains("limit"), "{error}");
    assert_eq!(store.recent(10).unwrap().len(), 1);
}

#[test]
fn malformed_tags_are_refused() {
    let (_dir, mut store) = open();
    let too_many: Vec<String> = (0..=MAX_TAGS).map(|i| format!("t{i}")).collect();
    assert!(store.write_note("x", &too_many, "a", 1).is_err());
    assert!(
        store
            .write_note("x", &tags(&["two words"]), "a", 1)
            .is_err()
    );
    assert!(store.write_note("x", &tags(&[""]), "a", 1).is_err());
    assert!(
        store
            .write_note("x", &tags(&[&"t".repeat(MAX_TAG_BYTES + 1)]), "a", 1)
            .is_err()
    );
}

#[test]
fn a_full_store_refuses_the_next_note_and_says_how_to_make_room() {
    let (_dir, mut store) = open();
    store
        .conn
        .execute(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
             INSERT INTO entries (kind, body, tags, author, created_at)
             SELECT 'note', 'filler', '', 'a', i FROM n",
            [MAX_NOTES],
        )
        .unwrap();
    let error = store
        .write_note("one more", &[], "a", 1)
        .unwrap_err()
        .to_string();
    assert!(error.contains("memory_delete"), "{error}");
}

#[test]
fn a_board_post_replaces_that_authors_previous_one_only() {
    let (_dir, mut store) = open();
    store.post_board("on the parser", "a", 10, 100).unwrap();
    store.post_board("on the docs", "b", 11, 100).unwrap();
    store.post_board("on the tests now", "a", 12, 100).unwrap();

    let board = store.board(20).unwrap();
    let lines: Vec<_> = board
        .iter()
        .map(|e| (e.author.as_str(), e.body.as_str()))
        .collect();
    assert_eq!(lines, [("a", "on the tests now"), ("b", "on the docs")]);
}

#[test]
fn a_board_post_is_gone_once_it_expires() {
    let (_dir, mut store) = open();
    store.post_board("briefly", "a", 10, 5).unwrap();
    assert_eq!(store.board(14).unwrap().len(), 1);
    assert!(store.board(15).unwrap().is_empty());
}

#[test]
fn a_board_ttl_outside_its_range_is_refused() {
    let (_dir, mut store) = open();
    for ttl in [0, -1, MAX_BOARD_TTL_SECS + 1] {
        assert!(store.post_board("x", "a", 1, ttl).is_err(), "{ttl}");
    }
}

#[test]
fn board_posts_are_not_notes() {
    let (_dir, mut store) = open();
    let id = store.post_board("working on search", "a", 1, 100).unwrap();
    assert!(store.recent(10).unwrap().is_empty());
    assert!(store.search("search", 10).unwrap().is_empty());
    assert!(!store.delete_note(id).unwrap());
}

#[test]
fn two_connections_to_one_file_see_each_others_writes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("p.db");
    let mut first = Store::open(&path).unwrap();
    let second = Store::open(&path).unwrap();
    first
        .write_note("written by the first", &[], "a", 1)
        .unwrap();
    assert_eq!(second.recent(10).unwrap().len(), 1);
}

#[test]
fn a_file_from_a_newer_helper_is_refused_untouched() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("p.db");
    drop(Store::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    drop(conn);
    let error = Store::open(&path).err().unwrap().to_string();
    assert!(error.contains("newer"), "{error}");
}

#[cfg(unix)]
#[test]
fn a_directory_the_store_creates_is_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, _store) = open();
    let mode = std::fs::metadata(dir.path().join("memory"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn many_openers_of_a_new_file_all_succeed() {
    // Each is the first tool call of an agent starting alongside the others.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("p.db");
    let openers: Vec<_> = (0..8)
        .map(|n| {
            let path = path.clone();
            std::thread::spawn(move || {
                let mut store = Store::open(&path).unwrap();
                store.write_note(&format!("from {n}"), &[], "a", n).unwrap();
            })
        })
        .collect();
    for opener in openers {
        opener.join().unwrap();
    }
    assert_eq!(Store::open(&path).unwrap().recent(50).unwrap().len(), 8);
}
