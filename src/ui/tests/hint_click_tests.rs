use super::common::*;
use crate::app::tests::app_with_fake_backend;
use crate::app::{App, Focus};
use crate::ui::hint_bar::{HintClick, hint_click_at, render_hint_bar};
use crate::ui::hint_text::{PREFIX_CHIP, normal_hint_literal, prefix_armed_hint_text};
use crate::ui::status_view::RepoInput;
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Color, text::Span};

/// x column where `needle` starts on the rendered hint row, measured in
/// display cells over exactly the text the renderer draws.
fn hint_x_of(app: &App, needle: &str) -> u16 {
    let (chip, text) = if app.interaction.prefix_armed {
        (PREFIX_CHIP, prefix_armed_hint_text(app))
    } else {
        (
            "",
            normal_hint_literal(app).replace(
                "<prefix>",
                &crate::app::leader_label_of(app.interaction.leader),
            ),
        )
    };
    let full = format!("{chip}{text}");
    let byte = full.find(needle).expect("needle must be on the hint row");
    Span::raw(&full[..byte]).width() as u16
}

const HINT_TEST_SCREEN: Rect = Rect::new(0, 0, 300, 40);
const HINT_ROW: u16 = 39;

#[test]
fn hint_click_resolves_commands_and_skips_nav_and_detach() {
    // Default state: FileList focus, status view — the row carries both
    // leader commands and nav segments.
    let app = app_with_fake_backend();

    let x = hint_x_of(&app, "t: new pane");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        Some(HintClick::Leader('t'))
    );
    let x = hint_x_of(&app, "/: search");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        Some(HintClick::Plain('/'))
    );
    let x = hint_x_of(&app, "j/k: navigate");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        None
    );
    let x = hint_x_of(&app, "q: detach");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        None,
        "detaching must never be one stray click away"
    );
}

#[test]
fn hint_click_agrees_with_the_rendered_buffer_not_just_the_builder() {
    // Independent cross-check: locate the label in the *rendered* buffer
    // (no shared width math with `hint_click_at`) and hit-test there. If
    // renderer and hit test ever segment differently, this drifts.
    let app = app_with_fake_backend();
    let mut terminal = Terminal::new(TestBackend::new(300, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                render_hint_bar(
                    &app,
                    plain_chrome(&RepoInput::default()),
                    Color::Yellow,
                    frame.area().width,
                ),
                frame.area(),
            )
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    // Scan cell-wise so the needle's index is a *column*, not a byte
    // offset — the row contains multi-byte arrows before the label.
    let cells: Vec<&str> = (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect();
    let x = (0..cells.len())
        .find(|&i| cells[i..].concat().starts_with("t: new pane"))
        .expect("label rendered") as u16;

    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        Some(HintClick::Leader('t'))
    );
}

#[test]
fn hint_click_misses_off_the_hint_row() {
    let app = app_with_fake_backend();
    let x = hint_x_of(&app, "t: new pane");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW - 1
        ),
        None
    );
}

#[test]
fn hint_click_armed_row_resolves_bare_followups_after_the_chip() {
    let mut app = app_with_fake_backend();
    app.interaction.prefix_armed = true;

    let x = hint_x_of(&app, "t: new pane");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        Some(HintClick::Plain('t'))
    );
    let x = hint_x_of(&app, "r: redraw");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        Some(HintClick::Plain('r'))
    );
    let x = hint_x_of(&app, "q: detach");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        None
    );
    let x = hint_x_of(&app, "esc: cancel");
    assert_eq!(
        hint_click_at(
            &app,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW
        ),
        None
    );
}

/// The armed row's remaining commands. They sit beside `x`/`p`/`r`, which have
/// always dispatched, so a click landing on one of these and doing nothing was
/// the row contradicting itself. `z` and `c` render only under their
/// availability predicates, so the state has to make them appear first.
#[test]
fn hint_click_armed_row_resolves_the_remaining_commands() {
    let mut app = app_with_fake_backend();
    app.interaction.prefix_armed = true;
    app.terminal.owns_size = false;
    app.terminal.recovery.insert(
        0,
        crate::runtime::terminal::PaneRecovery {
            state: "waiting_for_reset".to_string(),
            detail: None,
            deadline_epoch: None,
            attempt: 1,
        },
    );

    for (needle, key) in [
        ("u: reload config", 'u'),
        ("z: resize panes here", 'z'),
        ("c: cancel recovery", 'c'),
    ] {
        let x = hint_x_of(&app, needle);
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            Some(HintClick::Plain(key)),
            "`{needle}` names a command, so a click must dispatch it"
        );
    }
}

/// The match-stepping keys, on the row a search leaves behind. `shift+n` is the
/// one chord that resolves: its handler matches on the character, so the click
/// carries `N`.
#[test]
fn hint_click_resolves_the_search_match_keys() {
    let mut app = app_with_fake_backend();
    app.focus = Focus::DiffViewer;
    app.diff.search.query.set("foo");

    for (needle, key) in [("n: next match", 'n'), ("shift+n: prev match", 'N')] {
        let x = hint_x_of(&app, needle);
        assert_eq!(
            hint_click_at(
                &app,
                plain_chrome(&RepoInput::default()),
                HINT_TEST_SCREEN,
                x,
                HINT_ROW
            ),
            Some(HintClick::Plain(key)),
            "`{needle}` names a command, so a click must dispatch it"
        );
    }
}

#[test]
fn hint_click_none_on_modal_rows() {
    let mut swap = app_with_fake_backend();
    swap.interaction.begin_swap_target();
    assert!((0..HINT_TEST_SCREEN.width).all(|x| {
        hint_click_at(
            &swap,
            plain_chrome(&RepoInput::default()),
            HINT_TEST_SCREEN,
            x,
            HINT_ROW,
        )
        .is_none()
    }));
}
