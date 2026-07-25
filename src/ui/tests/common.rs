use crate::app::App;
use crate::app::tests::app_with_files;
use crate::config::LayoutConfig;
use crate::ui::chrome::Chrome;
use crate::ui::hint_bar::{hint_click_at, render_hint_bar};
use crate::ui::notice::render_notice_row;
use crate::ui::status_view::RepoInput;
use crate::ui::{draw, draw_empty};
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::Rect,
    style::{Color, Modifier},
};
use syntect::highlighting::ThemeSet;

pub(super) fn notice_text(app: &App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
    terminal
        .draw(|frame| frame.render_widget(render_notice_row(app, Color::Yellow), frame.area()))
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.width)
        .map(|x| buf[(x, 0)].symbol())
        .collect::<String>()
}

pub(super) fn test_workspace() -> crate::workspace::Workspace {
    let mut ws = crate::workspace::Workspace::new(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('f'),
        crossterm::event::KeyModifiers::CONTROL,
    ));
    ws.add(app_with_files(vec![]));
    ws
}

pub(super) fn plain_chrome(repo_input: &RepoInput) -> Chrome<'_> {
    Chrome {
        repo_paths: &[],
        active: 0,
        repo_input,
    }
}

pub(super) fn hint_text(app: &App) -> String {
    let repo_input = RepoInput::default();
    hint_text_with(app, plain_chrome(&repo_input))
}

pub(super) fn hint_text_with(app: &App, chrome: Chrome<'_>) -> String {
    let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(render_hint_bar(app, chrome, Color::Yellow), frame.area())
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn assert_inverted_cells_are_clickable(app: &App) {
    let repo_input = RepoInput::default();
    let chrome = plain_chrome(&repo_input);
    let mut terminal = Terminal::new(TestBackend::new(200, 1)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(render_hint_bar(app, chrome, Color::Yellow), frame.area())
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    let screen = Rect::new(0, 0, 200, 3);
    let mut inverted = 0;
    for x in 0..200u16 {
        let is_inverted = buf[(x, 0)].modifier.contains(Modifier::REVERSED);
        let is_clickable = hint_click_at(app, chrome, screen, x, 2).is_some();
        assert_eq!(
            is_inverted, is_clickable,
            "hint cell at column {x}: inverted={is_inverted} but clickable={is_clickable}"
        );
        inverted += is_inverted as u32;
    }
    assert!(
        inverted > 0,
        "at least one clickable key label must render inverted"
    );
}

pub(super) fn drawn_text(app: &mut App, tab_paths: &[String], active: usize) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    terminal
        .draw(|frame| {
            let tabs = Chrome {
                repo_paths: tab_paths,
                active,
                repo_input: &RepoInput::default(),
            };
            draw(
                frame,
                app,
                tabs,
                &ss,
                &ts,
                &LayoutConfig::default(),
                Color::Yellow,
            );
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn drawn_empty(
    repo_input: &RepoInput,
    notice: Option<&crate::app::Notice>,
    armed: bool,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(90, 12)).unwrap();
    let leader = crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Char('f'),
        crossterm::event::KeyModifiers::CONTROL,
    );
    terminal
        .draw(|frame| {
            let chrome = Chrome {
                repo_paths: &[],
                active: 0,
                repo_input,
            };
            draw_empty(frame, chrome, notice, leader, armed, false, Color::Yellow);
        })
        .unwrap();
    let buf = terminal.backend().buffer();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
