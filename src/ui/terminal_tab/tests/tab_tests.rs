use super::*;
use crate::runtime::terminal::PaneRecovery;
use crate::ui::terminal_tab::layout::terminal_layout;
use crate::ui::terminal_tab::render;
use crate::ui::terminal_tab::tab_bar::tab_target_at;
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color};

#[test]
fn tab_target_at_resolves_tabs_and_hidden_markers() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal.max_visible_normal = 2;
    for i in 0..4 {
        app.terminal
            .create_pane_with_now(None, Some(&format!("P{i}")))
            .unwrap();
    }
    // Creation leaves pane 3 active with a 2-pane window: [2, 4).
    let area = Rect::new(0, 0, 80, 20);
    let (tab_area, _) = terminal_layout(area).unwrap();
    let y = tab_area.y;
    // Segment 0 is the ` +2 ` marker → nearest hidden pane on the left.
    assert_eq!(
        tab_target_at(&app, area, tab_segment_x(&app, area, 0), y),
        Some(1)
    );
    // Segments 1 and 2 are the visible tabs for panes 2 and 3.
    assert_eq!(
        tab_target_at(&app, area, tab_segment_x(&app, area, 1), y),
        Some(2)
    );
    assert_eq!(
        tab_target_at(&app, area, tab_segment_x(&app, area, 2), y),
        Some(3)
    );
    // Past the last segment and off the tab row: no target.
    assert_eq!(tab_target_at(&app, area, tab_area.right() - 1, y), None);
    assert_eq!(
        tab_target_at(&app, area, tab_segment_x(&app, area, 1), y + 1),
        None
    );
}

#[test]
fn tab_target_at_right_marker_reveals_the_next_hidden_pane() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal.max_visible_normal = 2;
    for i in 0..4 {
        app.terminal
            .create_pane_with_now(None, Some(&format!("P{i}")))
            .unwrap();
    }
    // Jump back to pane 0: window slides to [0, 2), marker sits on the right.
    app.terminal.active = 0;
    app.terminal.sync_visible_window();
    let area = Rect::new(0, 0, 80, 20);
    let (tab_area, _) = terminal_layout(area).unwrap();

    // Segments: tab 0, tab 1, ` +2 ` marker → nearest hidden pane index 2.
    let x = tab_segment_x(&app, area, 2);
    assert_eq!(tab_target_at(&app, area, x, tab_area.y), Some(2));
}

#[test]
fn tab_target_agrees_with_the_rendered_buffer_not_just_the_builder() {
    // Independent cross-check: find the second tab's jump-key label in
    // the *rendered* buffer and hit-test at that column. Catches any
    // renderer vs hit-test segmentation drift the builder-based
    // position helper cannot see.
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("Alpha"))
        .unwrap();
    app.terminal
        .create_pane_with_now(None, Some("Beta"))
        .unwrap();
    let area = Rect::new(0, 0, 80, 20);
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
    terminal
        .draw(|frame| {
            render(frame, &app, area, Color::Yellow);
        })
        .unwrap();

    let (tab_area, _) = terminal_layout(area).unwrap();
    let buf = terminal.backend().buffer();
    let cells: Vec<&str> = (0..buf.area.width)
        .map(|x| buf[(x, tab_area.y)].symbol())
        .collect();
    let x = (0..cells.len())
        .find(|&i| cells[i..].concat().starts_with("^F 4 Beta"))
        .expect("second tab rendered") as u16;

    assert_eq!(tab_target_at(&app, area, x, tab_area.y), Some(1));
}

#[test]
fn tab_target_at_none_on_the_no_pane_legend() {
    let app = crate::app::tests::app_with_fake_backend();
    let area = Rect::new(0, 0, 80, 20);
    let (tab_area, _) = terminal_layout(area).unwrap();

    assert_eq!(tab_target_at(&app, area, tab_area.x + 2, tab_area.y), None);
}

#[test]
fn tab_bar_marks_hidden_panes_beyond_max_visible() {
    let mut app = crate::app::tests::app_with_fake_backend();
    for i in 0..5 {
        app.terminal
            .create_pane_with_now(None, Some(&format!("P{i}")))
            .unwrap();
    }
    assert_eq!(app.terminal.max_visible_normal, 4);
    let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, frame.area(), Color::Yellow);
        })
        .unwrap();

    let text = buffer_text(terminal.backend().buffer());
    assert!(
        text.contains('+'),
        "expected a hidden-pane count marker, got: {text}"
    );
}

#[test]
fn tab_bar_labels_panes_with_leader_digits_in_split_view() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("Alpha"))
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, frame.area(), Color::Yellow);
        })
        .unwrap();

    let text = buffer_text(terminal.backend().buffer());
    // The bare F-keys select project tabs, so the pane legend must name
    // the leader digit that actually reaches this pane.
    assert!(
        text.contains("^F 3 Alpha"),
        "split view must label the first pane with its <prefix> 3 jump key, got: {text}"
    );
    // Separating the digit also separates the strings: a bare `F3 Alpha`
    // legend can no longer hide inside the leader label, so this reads
    // directly rather than stripping the legitimate legend first.
    assert!(
        !text.contains("F3 Alpha"),
        "the bare F-key must not be advertised for panes, got: {text}"
    );
}

#[test]
fn tab_bar_labels_panes_with_digits_in_fullscreen() {
    // Fullscreen hides the viewer, so the pane legend switches to the
    // `<prefix> 1..8` digits that address panes there.
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("Alpha"))
        .unwrap();
    app.terminal
        .create_pane_with_now(None, Some("Beta"))
        .unwrap();
    app.terminal.fullscreen = crate::runtime::terminal::TerminalFullscreen::Grid;
    let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();

    terminal
        .draw(|frame| {
            render(frame, &app, frame.area(), Color::Yellow);
        })
        .unwrap();

    let text = buffer_text(terminal.backend().buffer());
    assert!(
        text.contains("1 Alpha") && text.contains("2 Beta"),
        "fullscreen must label panes with their <prefix> digits, got: {text}"
    );
    assert!(
        !text.contains("F3"),
        "fullscreen must not show the split-view F-key legend, got: {text}"
    );
}

#[test]
fn a_tab_label_carries_a_recovery_marker_only_while_one_is_reported() {
    let mut app = crate::app::tests::app_with_fake_backend();
    app.terminal
        .create_pane_with_now(None, Some("agent"))
        .unwrap();
    let pane = app.terminal.panes[0].id;
    let visible = 0..1;

    let plain = tab_segments(&app, visible.clone())[0].0.clone();
    assert!(plain.contains("agent"), "{plain}");
    assert!(
        !plain.contains('⏳'),
        "an unwatched pane must carry no marker"
    );

    app.terminal.recovery.insert(
        pane,
        PaneRecovery {
            state: "waiting_for_reset".to_string(),
            detail: Some("provider window closed".to_string()),
            deadline_epoch: Some(1_700_000_000),
            attempt: 3,
        },
    );

    let marked = tab_segments(&app, visible)[0].0.clone();
    assert!(marked.contains("agent"), "the title must survive: {marked}");
    assert!(marked.contains('⏳'), "{marked}");
    assert!(marked.contains("⚠3"), "{marked}");
    // The deadline is a wall-clock time, so the label carries a `HH:MM` colon
    // that the plain label did not.
    assert!(marked.contains(':'), "{marked}");
}

#[test]
fn a_long_title_loses_characters_before_a_recovery_marker_does() {
    let mut app = crate::app::tests::app_with_fake_backend();
    let long = "a-very-long-program-title-indeed";
    app.terminal.create_pane_with_now(None, Some(long)).unwrap();
    let pane = app.terminal.panes[0].id;
    app.terminal.recovery.insert(
        pane,
        PaneRecovery {
            state: "backoff".to_string(),
            detail: None,
            deadline_epoch: None,
            attempt: 7,
        },
    );

    let label = tab_segments(&app, 0..1)[0].0.clone();

    assert!(label.contains("⚠7"), "the marker must survive: {label}");
    assert!(label.contains('…'), "the title must be cut: {label}");
    assert!(!label.contains(long), "{label}");
}
