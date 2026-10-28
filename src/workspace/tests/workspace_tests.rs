use super::common::*;
use super::*;

#[test]
fn an_empty_workspace_has_no_active_project() {
    let ws = Workspace::new(test_leader());

    assert!(ws.active().is_none());
    assert!(ws.projects().is_empty());
}

#[test]
fn remembered_closed_projects_stay_within_the_cap() {
    // A long-lived process that opens and closes many repositories would
    // otherwise hold every session it had ever seen — and rescan that
    // history on every save.
    let mut ws = Workspace::new(test_leader());
    for i in 0..MAX_REMEMBERED + 5 {
        ws.add(project_at(&format!("/w/p{i}")));
        ws.close_repo(&format!("/w/p{i}"));
    }

    assert_eq!(ws.remembered.len(), MAX_REMEMBERED);
    // The most recently closed survives; the oldest is gone.
    assert!(
        ws.session_for(&format!("/w/p{}", MAX_REMEMBERED + 4))
            .is_some()
    );
    assert!(ws.session_for("/w/p0").is_none());
}

#[test]
fn closing_the_last_tab_leaves_the_empty_state() {
    // Quitting is no longer the only way out of the last project: the
    // empty screen is a real state nightcrow also starts in.
    let mut ws = workspace_on(&["/a"]);

    assert!(ws.close_repo("/a"));

    assert!(ws.active().is_none());
    assert!(!ws.close_repo("/a"), "nothing left to close");
}

#[test]
fn adding_a_project_appends_it_and_makes_it_active() {
    let mut ws = workspace_from(project_at("/a"));

    assert!(ws.add(project_at("/b")));

    assert_eq!(paths(&ws), vec!["/a", "/b"]);
    assert_eq!(ws.active().unwrap().git.repo_path, "/b");
}

#[test]
fn reaching_the_cap_rejects_addition_and_preserves_the_active_project() {
    let mut ws = workspace_from(project_at("/p0"));
    for i in 1..MAX_PROJECTS {
        assert!(ws.add(project_at(&format!("/p{i}"))));
    }
    assert_eq!(ws.projects().len(), MAX_PROJECTS);
    let active_before = ws.active().unwrap().git.repo_path.clone();

    assert!(!ws.add(project_at("/overflow")));

    assert_eq!(ws.projects().len(), MAX_PROJECTS);
    assert_eq!(ws.active().unwrap().git.repo_path, active_before);
    assert!(ws.index_of_repo("/overflow").is_none());
}

#[test]
fn closing_the_middle_tab_activates_the_following_tab() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));
    ws.add(project_at("/c"));
    ws.switch(1);

    assert!(ws.close_repo("/b"));

    assert_eq!(paths(&ws), vec!["/a", "/c"]);
    assert_eq!(ws.active().unwrap().git.repo_path, "/c");
}

#[test]
fn closing_the_last_tab_activates_the_previous_tab() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));

    assert!(ws.close_repo("/b"));

    assert_eq!(paths(&ws), vec!["/a"]);
    assert_eq!(ws.active().unwrap().git.repo_path, "/a");
}

#[test]
fn adding_a_project_discards_the_previous_projects_press() {
    // `add` makes the new project active, so the outgoing project's press
    // is released in place — same reasoning as `switch`.
    let mut ws = workspace_on(&["/a"]);
    ws.active_mut().unwrap().interaction.pending_mouse_press =
        Some((1, crossterm::event::MouseButton::Left, 1, 1));

    ws.add(project_at("/b"));

    assert!(ws.projects()[0].interaction.pending_mouse_press.is_none());
}

#[test]
fn switching_projects_discards_the_previous_pending_mouse_press() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));
    ws.switch(0);
    ws.active_mut().unwrap().interaction.pending_mouse_press =
        Some((1, crossterm::event::MouseButton::Left, 1, 1));

    ws.switch(1);

    // /a's press can never be paired now, so it is released where it
    // happened rather than left to match an unrelated release later.
    assert!(ws.projects()[0].interaction.pending_mouse_press.is_none());
}

#[test]
fn switching_to_the_same_index_preserves_a_pending_press() {
    // A no-op switch must not disturb an in-flight press/release pair.
    let mut ws = workspace_from(project_at("/a"));
    let press = Some((1, crossterm::event::MouseButton::Left, 1, 1));
    ws.active_mut().unwrap().interaction.pending_mouse_press = press;

    ws.switch(0);

    assert_eq!(ws.active().unwrap().interaction.pending_mouse_press, press);
}

#[test]
fn an_out_of_range_switch_does_not_change_the_active_project() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));

    ws.switch(9);

    assert_eq!(ws.active().unwrap().git.repo_path, "/b");
}

#[test]
fn confirming_the_active_project_clears_only_its_attention() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));
    for project in ws.projects_mut() {
        project.terminal.raise_attention();
    }

    ws.switch(0);

    assert!(!ws.projects()[0].terminal.has_unread_attention());
    assert!(ws.projects()[1].terminal.has_unread_attention());
}

#[test]
fn an_open_repository_is_found_by_path_or_returns_none() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));

    assert_eq!(ws.index_of_repo("/a"), Some(0));
    assert_eq!(ws.index_of_repo("/b"), Some(1));
    assert_eq!(ws.index_of_repo("/nope"), None);
}

#[test]
fn an_out_of_range_accent_wraps_around() {
    // The index arrives from the daemon, which takes it from a hand-editable
    // file — an index with no colour behind it must not reach the renderer.
    let mut ws = Workspace::new(test_leader());

    ws.set_accent_index(usize::MAX);

    assert_eq!(
        ws.current_accent(),
        crate::config::Accent::from_index(usize::MAX % crate::config::Accent::ALL.len()).color()
    );
}

#[test]
fn the_next_accent_advances_one_step_and_wraps_at_the_end() {
    let mut ws = Workspace::new(test_leader());

    ws.set_accent_index(0);
    assert_eq!(ws.next_accent_index(), 1);

    ws.set_accent_index(crate::config::Accent::ALL.len() - 1);
    assert_eq!(ws.next_accent_index(), 0);
}

#[test]
fn the_accent_is_preserved_without_projects() {
    // The empty screen is painted in the session's accent too, so it cannot
    // live on a project.
    let mut ws = Workspace::new(test_leader());

    ws.set_accent_index(3);

    assert!(ws.active().is_none());
    assert_eq!(
        ws.current_accent(),
        crate::config::Accent::from_index(3).color()
    );
}
