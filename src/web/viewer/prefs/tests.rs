use super::*;

#[test]
fn an_accent_round_trips_through_the_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nested").join("viewer.json");

    PrefsStore::at(path.clone()).set_accent(3);

    assert_eq!(PrefsStore::at(path).get().accent, 3);
}

#[test]
fn an_out_of_range_accent_wraps_instead_of_being_stored_as_given() {
    // The index comes from a browser, so it is input: storing it verbatim
    // would hand every later reader a value with no colour behind it.
    let dir = tempfile::TempDir::new().unwrap();
    let store = PrefsStore::at(dir.path().join("viewer.json"));

    let stored = store.set_accent(Accent::ALL.len() + 2);

    assert_eq!(stored.accent, 2);
    assert_eq!(store.get().accent, 2);
}

#[test]
fn a_corrupt_file_reads_as_defaults_rather_than_failing() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("viewer.json");
    std::fs::write(&path, "{not json").unwrap();

    assert_eq!(PrefsStore::at(path).get(), ViewerPrefs::default());
}

#[test]
fn a_missing_file_reads_as_defaults() {
    let dir = tempfile::TempDir::new().unwrap();

    let store = PrefsStore::at(dir.path().join("absent.json"));

    assert_eq!(store.get().accent, 0);
    assert_eq!(store.get().sidebar_width, DEFAULT_SIDEBAR_WIDTH);
}

#[test]
fn a_sidebar_width_round_trips_through_the_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("viewer.json");

    PrefsStore::at(path.clone()).set_sidebar_width(500);

    assert_eq!(PrefsStore::at(path).get().sidebar_width, 500);
}

#[test]
fn an_out_of_range_sidebar_width_clamps_instead_of_being_stored_as_given() {
    // The width comes from a browser drag, so it is input: a value past the
    // bounds would hand a later device a split with no diff pane, or one so
    // narrow the status letters clip.
    let dir = tempfile::TempDir::new().unwrap();
    let store = PrefsStore::at(dir.path().join("viewer.json"));

    assert_eq!(
        store
            .set_sidebar_width(MAX_SIDEBAR_WIDTH + 400)
            .sidebar_width,
        MAX_SIDEBAR_WIDTH
    );
    assert_eq!(store.set_sidebar_width(10).sidebar_width, MIN_SIDEBAR_WIDTH);
}

#[test]
fn a_width_outside_the_bounds_in_the_file_is_clamped_on_load() {
    // A hand-edited file must not smuggle a value past the bounds the write
    // path enforces — `get` would otherwise serve it and an accent-only
    // write would echo it back.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("viewer.json");
    std::fs::write(&path, r#"{"accent":0,"sidebar_width":9000}"#).unwrap();

    assert_eq!(PrefsStore::at(path).get().sidebar_width, MAX_SIDEBAR_WIDTH);
}

#[test]
fn an_active_repo_round_trips_through_the_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("viewer.json");

    PrefsStore::at(path.clone()).set_active_repo("/w/api".to_string());

    assert_eq!(
        PrefsStore::at(path).get().active_repo.as_deref(),
        Some("/w/api")
    );
}

#[test]
fn selecting_a_project_leaves_the_other_preferences_as_they_were() {
    // The three settings share one file and one lock; a write naming only the
    // project must not reset the look the user chose earlier.
    let dir = tempfile::TempDir::new().unwrap();
    let store = PrefsStore::at(dir.path().join("viewer.json"));
    store.update(Some(3), Some(500), None);

    let stored = store.set_active_repo("/w/api".to_string());

    assert_eq!(stored.accent, 3);
    assert_eq!(stored.sidebar_width, 500);
    assert_eq!(stored.active_repo.as_deref(), Some("/w/api"));
}

#[test]
fn a_write_naming_no_project_keeps_the_remembered_one() {
    // `None` means "leave it", not "clear it": every accent write would
    // otherwise forget which project the user was in.
    let dir = tempfile::TempDir::new().unwrap();
    let store = PrefsStore::at(dir.path().join("viewer.json"));
    store.set_active_repo("/w/api".to_string());

    let stored = store.set_accent(1);

    assert_eq!(stored.active_repo.as_deref(), Some("/w/api"));
}

#[test]
fn an_older_file_without_an_active_repo_loads_with_none() {
    // A `viewer.json` written before the field existed must still load, with
    // no project remembered rather than a failed read that drops the accent.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("viewer.json");
    std::fs::write(&path, r#"{"accent":3,"sidebar_width":500}"#).unwrap();

    let prefs = PrefsStore::at(path).get();
    assert_eq!(prefs.accent, 3);
    assert_eq!(prefs.active_repo, None);
}

#[test]
fn an_older_file_without_a_width_keeps_its_accent_and_defaults_the_width() {
    // A `viewer.json` written before the field existed must still load: the
    // container `#[serde(default)]` fills the missing width, not zero.
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("viewer.json");
    std::fs::write(&path, r#"{"accent":3}"#).unwrap();

    let prefs = PrefsStore::at(path).get();
    assert_eq!(prefs.accent, 3);
    assert_eq!(prefs.sidebar_width, DEFAULT_SIDEBAR_WIDTH);
}
