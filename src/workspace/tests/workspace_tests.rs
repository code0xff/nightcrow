use super::common::*;
use super::*;

#[test]
fn 빈_workspace는_활성_프로젝트가_없다() {
    let ws = Workspace::new(test_leader());

    assert!(ws.active().is_none());
    assert!(ws.projects().is_empty());
}

#[test]
fn 닫은_프로젝트_기억은_상한을_넘지_않는다() {
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
fn 마지막_탭을_닫으면_빈_상태가_된다() {
    // Quitting is no longer the only way out of the last project: the
    // empty screen is a real state nightcrow also starts in.
    let mut ws = workspace_on(&["/a"]);

    assert!(ws.close_repo("/a"));

    assert!(ws.active().is_none());
    assert!(!ws.close_repo("/a"), "nothing left to close");
}

#[test]
fn 프로젝트를_추가하면_끝에_붙고_활성이_된다() {
    let mut ws = workspace_from(project_at("/a"));

    assert!(ws.add(project_at("/b")));

    assert_eq!(paths(&ws), vec!["/a", "/b"]);
    assert_eq!(ws.active().unwrap().repo_path, "/b");
}

#[test]
fn 상한에_도달하면_추가를_거부하고_활성을_유지한다() {
    let mut ws = workspace_from(project_at("/p0"));
    for i in 1..MAX_PROJECTS {
        assert!(ws.add(project_at(&format!("/p{i}"))));
    }
    assert_eq!(ws.projects().len(), MAX_PROJECTS);
    let active_before = ws.active().unwrap().repo_path.clone();

    assert!(!ws.add(project_at("/overflow")));

    assert_eq!(ws.projects().len(), MAX_PROJECTS);
    assert_eq!(ws.active().unwrap().repo_path, active_before);
    assert!(ws.index_of_repo("/overflow").is_none());
}

#[test]
fn 가운데_탭을_닫으면_뒤_탭이_활성이_된다() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));
    ws.add(project_at("/c"));
    ws.switch(1);

    assert!(ws.close_repo("/b"));

    assert_eq!(paths(&ws), vec!["/a", "/c"]);
    assert_eq!(ws.active().unwrap().repo_path, "/c");
}

#[test]
fn 마지막_탭을_닫으면_앞_탭이_활성이_된다() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));

    assert!(ws.close_repo("/b"));

    assert_eq!(paths(&ws), vec!["/a"]);
    assert_eq!(ws.active().unwrap().repo_path, "/a");
}

#[test]
fn 프로젝트를_추가해도_이전_프로젝트의_press가_버려진다() {
    // `add` makes the new project active, so the outgoing project's press
    // is released in place — same reasoning as `switch`.
    let mut ws = workspace_on(&["/a"]);
    ws.active_mut().unwrap().pending_mouse_press =
        Some((1, crossterm::event::MouseButton::Left, 1, 1));

    ws.add(project_at("/b"));

    assert!(ws.projects()[0].pending_mouse_press.is_none());
}

#[test]
fn 전환하면_이전_프로젝트의_대기중인_마우스_press가_버려진다() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));
    ws.switch(0);
    ws.active_mut().unwrap().pending_mouse_press =
        Some((1, crossterm::event::MouseButton::Left, 1, 1));

    ws.switch(1);

    // /a's press can never be paired now, so it is released where it
    // happened rather than left to match an unrelated release later.
    assert!(ws.projects()[0].pending_mouse_press.is_none());
}

#[test]
fn 같은_인덱스로_전환하면_대기중인_press를_유지한다() {
    // A no-op switch must not disturb an in-flight press/release pair.
    let mut ws = workspace_from(project_at("/a"));
    let press = Some((1, crossterm::event::MouseButton::Left, 1, 1));
    ws.active_mut().unwrap().pending_mouse_press = press;

    ws.switch(0);

    assert_eq!(ws.active().unwrap().pending_mouse_press, press);
}

#[test]
fn 범위를_벗어난_전환은_활성을_바꾸지_않는다() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));

    ws.switch(9);

    assert_eq!(ws.active().unwrap().repo_path, "/b");
}

#[test]
fn 열린_저장소는_경로로_찾을_수_있고_없으면_none이다() {
    let mut ws = workspace_from(project_at("/a"));
    ws.add(project_at("/b"));

    assert_eq!(ws.index_of_repo("/a"), Some(0));
    assert_eq!(ws.index_of_repo("/b"), Some(1));
    assert_eq!(ws.index_of_repo("/nope"), None);
}
