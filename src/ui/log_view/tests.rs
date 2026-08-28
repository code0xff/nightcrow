use super::*;
use crate::git::diff::{ChangedFile, CommitEntry};
use git2::Oid;

fn entry(time: i64) -> CommitEntry {
    CommitEntry::new(
        Oid::ZERO_SHA1,
        "deadbee".to_string(),
        format!("c{time}"),
        "T".to_string(),
        time,
    )
}

#[test]
fn append_page_extends_commits_and_tracks_loaded_count() {
    let mut lv = LogView::default();
    lv.set_commits(vec![entry(0), entry(1)]);

    lv.append_page(vec![entry(2), entry(3)], 2);

    assert_eq!(lv.commits.len(), 4);
    assert_eq!(lv.loaded_count, 4);
    assert!(!lv.fully_loaded);
    assert!(!lv.pending_fetch);
}

#[test]
fn append_page_short_result_marks_fully_loaded() {
    let mut lv = LogView::default();
    lv.set_commits(vec![entry(0), entry(1)]);

    lv.append_page(vec![entry(2)], 3);

    assert_eq!(lv.commits.len(), 3);
    assert!(lv.fully_loaded);
}

#[test]
fn append_page_empty_result_marks_fully_loaded_without_extending() {
    let mut lv = LogView::default();
    lv.set_commits(vec![entry(0)]);

    lv.append_page(Vec::new(), 3);

    assert_eq!(lv.commits.len(), 1);
    assert_eq!(lv.loaded_count, 1);
    assert!(lv.fully_loaded);
    assert!(!lv.pending_fetch);
}

#[test]
fn mark_pending_is_idempotent() {
    let mut lv = LogView::default();
    assert!(lv.mark_pending());
    assert!(!lv.mark_pending());
    lv.clear_pending();
    assert!(lv.mark_pending());
}

#[test]
fn set_commits_resets_pagination_state() {
    let mut lv = LogView::default();
    lv.set_commits(vec![entry(0)]);
    lv.append_page(vec![entry(1)], 5);
    assert!(lv.fully_loaded);

    lv.set_commits(vec![entry(2), entry(3)]);
    assert_eq!(lv.loaded_count, 2);
    assert!(!lv.fully_loaded);
    assert!(!lv.pending_fetch);
}

fn named_entry(summary: &str) -> CommitEntry {
    CommitEntry::new(
        Oid::ZERO_SHA1,
        "deadbee".to_string(),
        summary.to_string(),
        "T".to_string(),
        0,
    )
}

#[test]
fn commit_filter_empty_query_includes_all_indices() {
    let mut lv = LogView::default();
    lv.set_commits(vec![entry(0), entry(1), entry(2)]);
    assert_eq!(lv.commits_filter_cache, vec![0, 1, 2]);
}

#[test]
fn commit_filter_substring_is_case_insensitive() {
    let mut lv = LogView::default();
    lv.set_commits(vec![
        named_entry("Fix Auth bug"),
        named_entry("feat: AUTH refresh"),
        named_entry("docs: readme"),
    ]);
    lv.commit_search_push('a');
    lv.commit_search_push('u');
    lv.commit_search_push('t');
    lv.commit_search_push('h');
    assert_eq!(lv.commits_filter_cache, vec![0, 1]);
}

#[test]
fn append_page_extends_filter_cache_for_matching_tail() {
    let mut lv = LogView::default();
    lv.set_commits(vec![named_entry("alpha"), named_entry("zulu")]);
    lv.commit_search_push('a');
    assert_eq!(lv.commits_filter_cache, vec![0]);

    lv.append_page(vec![named_entry("quill"), named_entry("apple")], 2);
    assert_eq!(lv.commits_filter_cache, vec![0, 3]);
}

#[test]
fn cancel_commit_search_clears_query_and_resets_cache() {
    let mut lv = LogView::default();
    lv.set_commits(vec![named_entry("alpha"), named_entry("zulu")]);
    lv.start_commit_search();
    lv.commit_search_push('a');
    assert_eq!(lv.commits_filter_cache, vec![0]);

    lv.cancel_commit_search();
    assert!(!lv.commit_search_active);
    assert!(lv.commit_search_query.is_empty());
    assert_eq!(lv.commits_filter_cache, vec![0, 1]);
}

#[test]
fn confirm_commit_search_hides_bar_but_keeps_filter() {
    let mut lv = LogView::default();
    lv.set_commits(vec![named_entry("alpha"), named_entry("zulu")]);
    lv.start_commit_search();
    lv.commit_search_push('a');

    let collapsed_to_cancel = lv.confirm_commit_search();
    assert!(!collapsed_to_cancel);
    assert!(!lv.commit_search_active);
    assert_eq!(lv.commit_search_query.as_str(), "a");
    assert_eq!(lv.commits_filter_cache, vec![0]);
}

#[test]
fn confirm_commit_search_on_empty_query_collapses_to_cancel() {
    let mut lv = LogView::default();
    lv.set_commits(vec![named_entry("alpha")]);
    lv.start_commit_search();

    let collapsed_to_cancel = lv.confirm_commit_search();
    assert!(collapsed_to_cancel);
    assert!(!lv.commit_search_active);
}

#[test]
fn set_commit_files_seeds_filter_cache_under_active_query() {
    let mut lv = LogView::default();
    lv.file_search_push('r');
    lv.set_commit_files(vec![
        ChangedFile::unstaged_only("readme.md".into(), crate::git::diff::StatusKind::Modified),
        ChangedFile::unstaged_only("src/lib.rs".into(), crate::git::diff::StatusKind::Modified),
    ]);
    assert_eq!(lv.commit_files_filter_cache, vec![0, 1]);

    lv.file_search_push('e');
    lv.file_search_push('a');
    // "readme.md" contains "rea"; "src/lib.rs" does not.
    assert_eq!(lv.commit_files_filter_cache, vec![0]);
}

#[test]
fn reset_drill_down_clears_file_search_state() {
    let mut lv = LogView::default();
    lv.enter_drill_down();
    lv.set_commit_files(vec![ChangedFile::unstaged_only(
        "readme.md".into(),
        crate::git::diff::StatusKind::Modified,
    )]);
    lv.start_file_search();
    lv.file_search_push('r');
    assert!(lv.file_search_active);
    assert_eq!(lv.commit_files_filter_cache, vec![0]);

    lv.reset_drill_down();
    assert!(!lv.drill_down);
    assert!(!lv.file_search_active);
    assert!(lv.file_search_query.is_empty());
    assert!(lv.commit_files_filter_cache.is_empty());
}
