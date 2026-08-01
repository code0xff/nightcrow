use super::helpers::*;
use crate::app;
use crate::app::tests::{app_with_fake_backend, app_with_files};
use crate::application::input::dispatch::{KeyOutcome, ProjectRequest};
use crate::application::input::mouse::{dispatch_mouse, handle_mouse};
use crate::workspace::Workspace;
use crossterm::event::MouseEventKind;

#[test]
fn clicking_a_project_tab_asks_the_workspace_to_switch() {
    let mut app = app_with_fake_backend();
    let tabs = vec!["/w/api".to_string(), "/w/web".to_string()];
    // Column 0 of row 0 is the first tab; a click there is the pointer
    // equivalent of pressing F1.
    let outcome = handle_mouse(
        &mut app,
        crate::ui::Chrome {
            repo_paths: &tabs,
            active: 1,
            repo_input: &crate::ui::status_view::RepoInput::default(),
        },
        mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            0,
            0,
        ),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
    );

    assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::Switch(0)));
}

#[test]
fn the_dialog_still_lets_a_pending_release_through() {
    // A modal opening between press and release must not strand the
    // pending slot: the pane that saw the press has to see the release, and
    // a leftover slot would pair with a later unrelated one.
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    // Only a pane whose program asked for mouse reports records a pending
    // press, so opt it in.
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");
    let mut ws = Workspace::new(leader());
    ws.add(app);
    let down = MouseEventKind::Down(crossterm::event::MouseButton::Left);
    let up = MouseEventKind::Up(crossterm::event::MouseButton::Left);
    let tabs = test_tabs();

    dispatch_mouse(
        &mut ws,
        test_tab_view(&tabs),
        mouse(down, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
        true,
    );
    assert!(
        ws.active()
            .unwrap()
            .interaction
            .pending_mouse_press
            .is_some()
    );

    ws.start_repo_input();
    dispatch_mouse(
        &mut ws,
        test_tab_view(&tabs),
        mouse(up, rect.x, rect.y),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
        true,
    );

    assert!(
        ws.active()
            .unwrap()
            .interaction
            .pending_mouse_press
            .is_none(),
        "the dialog must not swallow the release"
    );
}

#[test]
fn switching_projects_releases_a_pending_press_to_its_own_pane() {
    // The old PTY is still alive; without a release it sits in a drag or
    // selection state forever, since drag reports are never forwarded.
    let (mut app, areas) = app_with_two_panes_and_areas();
    let (id, rect) = areas[0];
    app.terminal
        .emulators
        .get_mut(&id)
        .unwrap()
        .process(b"\x1b[?1000h\x1b[?1006h");
    let mut ws = Workspace::new(leader());
    ws.add(app);
    let tabs = test_tabs();
    dispatch_mouse(
        &mut ws,
        test_tab_view(&tabs),
        mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            rect.x,
            rect.y,
        ),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
        true,
    );
    assert!(
        ws.active()
            .unwrap()
            .interaction
            .pending_mouse_press
            .is_some()
    );

    ws.add(app_with_files(vec!["b.rs"]));

    let old = &ws.projects()[0];
    assert!(old.interaction.pending_mouse_press.is_none());
    assert_eq!(
        backend_payloads(old),
        vec![b"\x1b[<0;1;1M".to_vec(), b"\x1b[<0;1;1m".to_vec()],
        "the pane must see its button-up, not just lose the record"
    );
}

#[test]
fn clicking_the_empty_screen_open_hint_raises_the_dialog() {
    // It is the only action that screen offers, so it must work by pointer
    // as well as by key — and it renders inverted, advertising as much.
    let mut ws = Workspace::new(leader());
    let tabs: Vec<String> = Vec::new();
    let label = app::leader_label_of(leader());
    let x = (0..MOUSE_TEST_SCREEN.width)
        .find(|&x| {
            crate::ui::empty_hint_click_at(
                MOUSE_TEST_SCREEN,
                &label,
                false,
                true,
                x,
                MOUSE_TEST_SCREEN.height - 1,
            )
            .is_some()
        })
        .expect("the open hint is clickable");

    let outcome = dispatch_mouse(
        &mut ws,
        test_tab_view(&tabs),
        mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            x,
            MOUSE_TEST_SCREEN.height - 1,
        ),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
        true,
    );

    assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::OpenDialog));
}

#[test]
fn clicking_the_open_hint_while_armed_disarms_the_prefix() {
    // The armed row lays out differently (chip plus bare keys), so the hit
    // test must measure that layout — and the click must disarm, or the
    // next key after the dialog closes resolves as a stale follow-up.
    let mut ws = Workspace::new(leader());
    ws.arm_prefix();
    let tabs: Vec<String> = Vec::new();
    let label = app::leader_label_of(leader());
    let row = MOUSE_TEST_SCREEN.height - 1;
    let x = (0..MOUSE_TEST_SCREEN.width)
        .find(|&x| {
            matches!(
                crate::ui::empty_hint_click_at(MOUSE_TEST_SCREEN, &label, true, true, x, row),
                Some(crate::ui::HintClick::Plain('o'))
            )
        })
        .expect("the armed open hint is clickable");

    let outcome = dispatch_mouse(
        &mut ws,
        test_tab_view(&tabs),
        mouse(
            MouseEventKind::Down(crossterm::event::MouseButton::Left),
            x,
            row,
        ),
        MOUSE_TEST_SCREEN,
        &crate::config::LayoutConfig::default(),
        true,
    );

    assert_eq!(outcome, KeyOutcome::Project(ProjectRequest::OpenDialog));
    assert!(!ws.prefix_armed(), "the click must disarm the prefix");
}

#[test]
fn the_empty_hint_is_inert_when_mouse_capture_is_disabled() {
    // The row renders plain in that case, and a browser mouse event still
    // reaches this path — a label that does not advertise itself as
    // clickable must not act like one.
    let label = app::leader_label_of(leader());
    let row = MOUSE_TEST_SCREEN.height - 1;

    assert!((0..MOUSE_TEST_SCREEN.width).all(|x| {
        crate::ui::empty_hint_click_at(MOUSE_TEST_SCREEN, &label, false, false, x, row).is_none()
    }));
}
