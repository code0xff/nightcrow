//! The chrome with the project tabs down the left (`[layout] tabs = "left"`).

use super::common::*;
use crate::app::tests::app_with_files;
use crate::config::TabStrip;
use crate::ui::chrome::Chrome;
use crate::ui::project_tab_at;
use crate::ui::status_view::RepoInput;
use ratatui::layout::Rect;

#[test]
fn a_left_strip_stacks_the_tabs_beside_a_narrower_body() {
    // `[layout] tabs = "left"`: the projects run down the first column, the
    // top row belongs to the body, and the notice and hint rows keep the
    // whole width underneath both.
    let paths = vec!["/w/api".to_string(), "/w/web".to_string()];
    let mut app = app_with_files(vec!["a.rs"]);

    let text = drawn_text_in(&mut app, &paths, 1, TabStrip::Left);
    let rows: Vec<&str> = text.lines().collect();

    assert!(rows[0].starts_with(" F1 api"), "row 0: {:?}", rows[0]);
    assert!(rows[1].starts_with(" F2 web"), "row 1: {:?}", rows[1]);
    // The body starts where the strip ends, on the very first row.
    let strip = crate::ui::project_tab::STRIP_WIDTH as usize;
    let body_top = &rows[0][strip..];
    assert!(
        body_top.trim_start().starts_with('┌') || body_top.contains('─'),
        "the body's top border shares row 0 with the strip: {body_top:?}"
    );
}

#[test]
fn project_tab_at_follows_the_strip_down_the_left() {
    let paths = vec!["/w/api".to_string(), "/w/web".to_string()];
    let screen = Rect::new(0, 0, 120, 20);
    let tabs = Chrome {
        repo_paths: &paths,
        attention: &[],
        attention_bright: true,
        active: 0,
        repo_input: &RepoInput::default(),
        strip: TabStrip::Left,
    };

    assert_eq!(project_tab_at(tabs, screen, 3, 0), Some(0));
    assert_eq!(project_tab_at(tabs, screen, 3, 1), Some(1));
    // Beside the strip is the body, whatever the row.
    let strip = crate::ui::project_tab::STRIP_WIDTH;
    assert_eq!(project_tab_at(tabs, screen, strip, 0), None);
}
