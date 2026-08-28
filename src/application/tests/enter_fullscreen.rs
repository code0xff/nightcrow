//! `Enter` key routing for the diff pane zoom.

use super::helpers::*;
use crate::app::Focus;
use crate::app::tests::app_with_files;
use crate::application::input::dispatch::handle_key;
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn enter_in_diff_viewer_toggles_diff_fullscreen() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.focus = Focus::DiffViewer;

    let _ = handle_key(&mut app, press(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        app.git.view.diff.fullscreen,
        "Enter must zoom the diff pane"
    );

    let _ = handle_key(&mut app, press(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        !app.git.view.diff.fullscreen,
        "a second Enter must exit the zoom"
    );
}
