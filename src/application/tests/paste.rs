use super::helpers::*;
use crate::app::Focus;
use crate::app::tests::app_with_files;
use crate::application::input::dispatch::handle_key;
use crate::application::input::paste::{dispatch_paste, handle_paste};

#[test]
fn paste_while_prefix_armed_cancels_prefix() {
    let mut app = app_with_terminal_pane();
    let _ = handle_key(&mut app, leader());
    assert!(app.prefix_armed());

    handle_paste(&mut app, "hello");

    assert!(
        !app.prefix_armed(),
        "a paste must resolve (cancel) the armed prefix"
    );
}

#[test]
fn terminal_paste_wraps_only_when_bracketed_mode_enabled() {
    let mut app = app_with_terminal_pane();
    // The running program enables bracketed paste (DECSET 2004).
    for emulator in app.terminal.emulators.values_mut() {
        emulator.process(b"\x1b[?2004h");
    }

    handle_paste(&mut app, "hi");

    assert_eq!(
        backend_payloads(&app).concat(),
        b"\x1b[200~hi\x1b[201~".to_vec(),
        "paste must be bracketed when the program enabled DECSET 2004"
    );
}

#[test]
fn terminal_paste_sends_raw_when_bracketed_mode_disabled() {
    let mut app = app_with_terminal_pane();

    handle_paste(&mut app, "hi");

    assert_eq!(
        backend_payloads(&app).concat(),
        b"hi".to_vec(),
        "without DECSET 2004 the markers must not be sent as literal input"
    );
}

#[test]
fn handle_paste_into_file_search_strips_control_chars() {
    // Regression for e21c449 + 4084760: paste into the file-search
    // overlay drops control characters (newlines, tabs, bells) before
    // appending to the query.
    let mut app = app_with_files(vec!["alpha.rs", "beta.rs"]);
    app.focus = Focus::FileList;
    app.start_search();

    handle_paste(&mut app, "al\nph\ta\x07");

    assert_eq!(app.status_view.search_query.as_str(), "alpha");
}

#[test]
fn handle_paste_into_diff_search_strips_control_chars() {
    let mut app = app_with_files(vec!["alpha.rs"]);
    app.focus = Focus::DiffViewer;
    app.diff.start_search();

    handle_paste(&mut app, "fn\rname\x08");

    assert_eq!(app.diff.search.query.as_str(), "fnname");
}

#[test]
fn paste_into_the_dialog_strips_control_chars() {
    let mut ws = workspace_on(&["/a"]);
    ws.start_repo_input();
    // `start_repo_input` prefills with the active repo path, and
    // `repo_input_push` preserves existing content, so reset first.
    ws.repo_input.buf.clear();

    dispatch_paste(&mut ws, "/tmp\n/repo\x07");

    assert_eq!(ws.repo_input.buf, "/tmp/repo");
}
