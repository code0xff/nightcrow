//! Remembering which panel each project was left maximized in.

use super::*;

fn panels(list: &[RepoMaximized]) -> Vec<(&str, MaximizedPanel)> {
    list.iter()
        .map(|entry| (entry.repo.as_str(), entry.panel))
        .collect()
}

#[test]
fn a_project_gets_back_the_panel_it_was_left_in() {
    let mut list = Vec::new();
    remember(&mut list, "/a", Some(MaximizedPanel::Terminal));
    remember(&mut list, "/b", Some(MaximizedPanel::Files));

    assert_eq!(panel_of(&list, "/a"), Some(MaximizedPanel::Terminal));
    assert_eq!(panel_of(&list, "/b"), Some(MaximizedPanel::Files));
    assert_eq!(panel_of(&list, "/never-opened"), None);
}

#[test]
fn maximizing_again_replaces_rather_than_stacks() {
    let mut list = Vec::new();
    remember(&mut list, "/a", Some(MaximizedPanel::Files));
    remember(&mut list, "/a", Some(MaximizedPanel::Terminal));

    assert_eq!(panels(&list), [("/a", MaximizedPanel::Terminal)]);
}

/// Nothing maximized is the absence of an entry, so un-maximizing every project
/// leaves an empty list rather than a row per project saying "no".
#[test]
fn un_maximizing_forgets_the_project() {
    let mut list = Vec::new();
    remember(&mut list, "/a", Some(MaximizedPanel::Terminal));

    remember(&mut list, "/a", None);

    assert!(list.is_empty());
    assert_eq!(panel_of(&list, "/a"), None);
}

#[test]
fn un_maximizing_a_project_with_no_entry_changes_nothing() {
    let mut list = vec![RepoMaximized {
        repo: "/a".into(),
        panel: MaximizedPanel::Files,
    }];

    remember(&mut list, "/never-maximized", None);

    assert_eq!(panels(&list), [("/a", MaximizedPanel::Files)]);
}

#[test]
fn the_most_recently_touched_project_is_first() {
    let mut list = Vec::new();
    remember(&mut list, "/a", Some(MaximizedPanel::Files));
    remember(&mut list, "/b", Some(MaximizedPanel::Files));
    remember(&mut list, "/a", Some(MaximizedPanel::Terminal));

    assert_eq!(
        panels(&list),
        [
            ("/a", MaximizedPanel::Terminal),
            ("/b", MaximizedPanel::Files)
        ],
    );
}

/// Or the file grows a row for every repository ever opened.
#[test]
fn the_oldest_projects_are_dropped_past_the_cap() {
    let mut list = Vec::new();
    for i in 0..MAX_REMEMBERED_MAXIMIZED + 10 {
        remember(&mut list, &format!("/repo{i}"), Some(MaximizedPanel::Files));
    }

    assert_eq!(list.len(), MAX_REMEMBERED_MAXIMIZED);
    assert_eq!(
        list[0].repo,
        format!("/repo{}", MAX_REMEMBERED_MAXIMIZED + 9)
    );
    assert_eq!(panel_of(&list, "/repo0"), None, "the oldest went");
}

#[test]
fn a_panel_the_client_made_up_is_refused_rather_than_guessed() {
    assert_eq!(MaximizedPanel::parse("files"), Some(MaximizedPanel::Files));
    assert_eq!(
        MaximizedPanel::parse("terminal"),
        Some(MaximizedPanel::Terminal)
    );
    for made_up in ["", "none", "Terminal", "diff", "../etc"] {
        assert_eq!(MaximizedPanel::parse(made_up), None, "{made_up}");
    }
}

/// The wire form is what the client reads back, so the two directions have to
/// agree or a stored panel would come back as one the client cannot apply.
#[test]
fn every_panel_survives_a_round_trip_through_its_wire_form() {
    for panel in [MaximizedPanel::Files, MaximizedPanel::Terminal] {
        assert_eq!(MaximizedPanel::parse(panel.as_str()), Some(panel));
    }
}
