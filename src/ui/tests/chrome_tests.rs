use super::common::*;
use crate::app::NoticeKind;
use crate::app::tests::{app_with_fake_backend, app_with_files};
use crate::config::LayoutConfig;
use crate::runtime::terminal::TerminalFullscreen;
use crate::ui::chrome::Chrome;
use crate::ui::status_view::RepoInput;
use crate::ui::{draw, home_relative_path, main_content_constraints, project_tab_at};
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::{Constraint, Rect},
    style::Color,
};
use syntect::highlighting::ThemeSet;

#[test]
fn the_empty_screen_names_the_only_two_things_that_work() {
    let text = drawn_empty(&RepoInput::default(), None, false);

    assert!(text.contains("no project open"), "got: {text}");
    assert!(text.contains("^F o: open project"), "got: {text}");
    assert!(text.contains("^F q: quit"), "got: {text}");
}

#[test]
fn the_empty_screen_shows_the_prefix_chip_when_armed() {
    // Pressing the leader with no project open has to look like it did
    // something, or it reads as a dead key.
    let text = drawn_empty(&RepoInput::default(), None, true);

    assert!(text.contains("PREFIX"), "got: {text}");
    assert!(text.contains("o: open project"), "got: {text}");
    assert!(text.contains("esc: cancel"), "got: {text}");
}

#[test]
fn the_empty_screen_shows_the_dialog_and_its_rejection() {
    // The dialog and its notice are the reason the empty screen keeps its
    // chrome at all — a rejected path must still report why.
    let repo_input = RepoInput {
        active: true,
        buf: "/definitely/not/here".to_string(),
        prefilled: false,
    };
    let notice = crate::app::Notice::new(NoticeKind::RepoInput, "no such directory");

    let text = drawn_empty(&repo_input, Some(&notice), false);

    assert!(text.contains("repo: /definitely/not/here"), "got: {text}");
    assert!(text.contains("no such directory"), "got: {text}");
}

#[test]
fn the_project_tab_row_survives_every_fullscreen_mode() {
    // Chrome is rendered before the layout branches precisely so no view
    // mode can strand the user without knowing which project they are in.
    let paths = vec!["/w/api".to_string(), "/w/web".to_string()];

    let mut app = app_with_fake_backend();
    assert!(drawn_text(&mut app, &paths, 0).contains("F2 web"), "split");

    let mut app = app_with_fake_backend();
    app.terminal.fullscreen = TerminalFullscreen::Grid;
    assert!(
        drawn_text(&mut app, &paths, 0).contains("F2 web"),
        "terminal fullscreen"
    );

    let mut app = app_with_files(vec!["a.rs"]);
    app.list_fullscreen = true;
    assert!(
        drawn_text(&mut app, &paths, 0).contains("F2 web"),
        "list fullscreen"
    );

    let mut app = app_with_files(vec!["a.rs"]);
    app.diff.fullscreen = true;
    assert!(
        drawn_text(&mut app, &paths, 0).contains("F2 web"),
        "diff fullscreen"
    );
}

#[test]
fn project_tab_at_matches_the_rendered_row() {
    // The hit test derives from `chrome_rows` like `draw` does, so a click
    // on a tab's glyphs must resolve to that tab.
    let mut app = app_with_files(vec!["a.rs"]);
    let paths = vec!["/w/api".to_string(), "/w/web".to_string()];
    let screen = Rect::new(0, 0, 120, 20);
    let text = drawn_text(&mut app, &paths, 0);
    let first_row = text.lines().next().unwrap();
    let web_x = first_row.find("F2 web").expect("second tab rendered") as u16;

    let tabs = Chrome {
        repo_paths: &paths,
        active: 0,
        repo_input: &RepoInput::default(),
    };
    assert_eq!(project_tab_at(tabs, screen, 0, 0), Some(0));
    assert_eq!(project_tab_at(tabs, screen, web_x, 0), Some(1));
    // Row 1 is the body, not the tab row.
    assert_eq!(project_tab_at(tabs, screen, web_x, 1), None);
}

#[test]
fn panels_advertise_the_leader_digit_not_the_bare_f_key() {
    // The bare F-key row selects project tabs, so a panel legend reading
    // `F1 Files` would name a key that switches projects instead of
    // focusing the panel.
    let mut app = app_with_files(vec!["a.rs"]);
    let tab_paths = vec![".".to_string()];
    let mut terminal = Terminal::new(TestBackend::new(120, 20)).unwrap();
    let ss = two_face::syntax::extra_newlines();
    let ts = ThemeSet::load_defaults();
    terminal
        .draw(|frame| {
            draw(
                frame,
                &mut app,
                Chrome {
                    repo_paths: &tab_paths,
                    active: 0,
                    repo_input: &RepoInput::default(),
                },
                &ss,
                &ts,
                &LayoutConfig::default(),
                Color::Yellow,
            );
        })
        .unwrap();

    let buf = terminal.backend().buffer();
    let text: String = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        text.contains("^F1 Files"),
        "file list must advertise its leader digit, got: {text}"
    );
    // The `Ctrl+F` leader label ("^F") ends in the letter F, so the legit
    // "^F1 Files" legend contains "F1 Files" as a substring. Strip it before
    // asserting the bare function-key legend never appears on its own.
    assert!(
        !text.replace("^F1 Files", "").contains("F1 Files"),
        "the bare F-key must not be advertised for panels, got: {text}"
    );
}

#[test]
fn home_relative_strips_home_prefix_and_trailing_slash() {
    let home = dirs::home_dir().expect("home dir for test host");
    let home_str = home.to_str().unwrap();
    let nested = format!("{home_str}/projects/foo/");
    assert_eq!(home_relative_path(&nested), "~/projects/foo");
}

#[test]
fn home_relative_keeps_paths_outside_home_unchanged() {
    // Trailing slash still trimmed for compactness, but the body is
    // returned verbatim when the home prefix doesn't match.
    assert_eq!(home_relative_path("/tmp/repo/"), "/tmp/repo");
    assert_eq!(home_relative_path("/var/code"), "/var/code");
}

#[test]
fn main_content_split_preserves_lower_panel_at_high_upper_ratio() {
    let cfg = LayoutConfig {
        upper_pct: 99,
        file_list_pct: 25,
    };

    assert_eq!(
        main_content_constraints(&cfg),
        [Constraint::Percentage(99), Constraint::Percentage(1)]
    );
}
