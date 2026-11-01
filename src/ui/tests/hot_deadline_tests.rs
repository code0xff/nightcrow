//! Memoization of the next hot-file fade deadline: the event loop asks every
//! tick, so the scan must only repeat when the inputs move or a transition
//! passes.

use crate::app::tests::app_with_files;
use crate::git::diff::{ChangedFile, StatusKind};
use crate::ui::file_list::next_hot_deadline_for_app;
use crate::ui::status_view::StatusView;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

const WINDOW: Duration = Duration::from_secs(15);

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn view_with(files: &[&str], mtimes: &[(&str, u64)]) -> StatusView {
    let mut view = StatusView::default();
    view.set_files(
        files
            .iter()
            .map(|path| ChangedFile::unstaged_only((*path).to_string(), StatusKind::Modified))
            .collect(),
    );
    view.recompute_filter();
    for (path, secs) in mtimes {
        view.hot_table.insert((*path).to_string(), at(*secs));
    }
    view.invalidate_hot_deadline();
    view
}

#[test]
fn the_earliest_transition_among_visible_files_wins() {
    let view = view_with(
        &["a.rs", "b.rs"],
        &[("a.rs", 100), ("b.rs", 98), ("hidden.rs", 99)],
    );

    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(103)));
}

#[test]
fn a_filtered_out_file_does_not_contribute_a_deadline() {
    let mut view = view_with(&["a.rs", "b.rs"], &[("a.rs", 100), ("b.rs", 98)]);
    view.search_query.set("a.rs");
    view.recompute_filter();

    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));
}

#[test]
fn the_deadline_is_reused_until_it_is_invalidated() {
    let mut view = view_with(&["a.rs"], &[("a.rs", 100)]);

    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));
    // A table change that bypasses the invalidation hook leaves the memoized
    // value in place: the scan must not repeat per tick.
    view.hot_table.clear();
    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));

    view.invalidate_hot_deadline();
    assert_eq!(view.next_hot_deadline(at(102), WINDOW), None);
}

#[test]
fn a_passed_deadline_recomputes_the_next_transition() {
    let view = view_with(&["a.rs"], &[("a.rs", 100)]);

    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));
    assert_eq!(view.next_hot_deadline(at(105), WINDOW), Some(at(115)));
    assert_eq!(view.next_hot_deadline(at(115), WINDOW), None);
}

#[test]
fn a_clock_rollback_recomputes_the_earlier_transition() {
    let view = view_with(&["a.rs"], &[("a.rs", 100)]);

    assert_eq!(view.next_hot_deadline(at(106), WINDOW), Some(at(115)));
    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));
}

#[test]
fn a_changed_hot_window_recomputes() {
    let view = view_with(&["a.rs"], &[("a.rs", 90)]);

    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));
    assert_eq!(
        view.next_hot_deadline(at(102), Duration::from_secs(20)),
        Some(at(110))
    );
}

#[test]
fn replacing_the_file_list_drops_the_cached_deadline() {
    let mut view = view_with(&["a.rs"], &[("a.rs", 100)]);
    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));

    view.hot_table.insert("a.rs".to_string(), at(200));
    view.set_files(vec![ChangedFile::unstaged_only(
        "a.rs".to_string(),
        StatusKind::Modified,
    )]);

    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(205)));
}

#[test]
fn recomputing_the_filter_drops_the_cached_deadline() {
    let mut view = view_with(&["a.rs"], &[("a.rs", 100)]);
    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(105)));

    view.hot_table.insert("a.rs".to_string(), at(200));
    view.recompute_filter();

    assert_eq!(view.next_hot_deadline(at(102), WINDOW), Some(at(205)));
}

#[test]
fn merging_new_mtimes_invalidates_the_app_level_deadline() {
    let mut app = app_with_files(vec!["a.rs", "b.rs"]);
    let base = at(1000);

    app.merge_hot_table(&HashMap::from([("a.rs".to_string(), base)]));
    let now = base + Duration::from_secs(2);
    assert_eq!(
        next_hot_deadline_for_app(&app, now),
        Some(base + Duration::from_secs(5))
    );

    app.merge_hot_table(&HashMap::from([(
        "b.rs".to_string(),
        base + Duration::from_secs(100),
    )]));

    assert_eq!(
        next_hot_deadline_for_app(&app, now),
        Some(base + Duration::from_secs(105))
    );
}

#[test]
fn non_status_modes_report_no_deadline() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.merge_hot_table(&HashMap::from([("a.rs".to_string(), at(1000))]));
    app.git.view.mode = crate::app::ViewMode::Log;

    assert_eq!(next_hot_deadline_for_app(&app, at(1002)), None);
}
