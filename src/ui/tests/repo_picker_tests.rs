use super::common::*;
use crate::ui::hint_bar::repo_input_line;
use crate::ui::status_view::RepoInput;
use crate::workspace::PathTree;
use ratatui::style::Color;
use tempfile::TempDir;

/// The dialog's state with the browser open on a temp directory holding `dirs`.
fn browsing(dirs: &[&str]) -> (TempDir, RepoInput) {
    let root = TempDir::new().expect("a temp dir");
    for d in dirs {
        std::fs::create_dir(root.path().join(d)).expect("create dir");
    }
    let buf = std::fs::canonicalize(root.path())
        .expect("canonical temp path")
        .to_str()
        .expect("a UTF-8 temp path")
        .to_string();
    let picker = PathTree::open(&buf).expect("a readable root");
    (
        root,
        RepoInput {
            active: true,
            buf,
            prefilled: false,
            candidates: Vec::new(),
            picker: Some(picker),
        },
    )
}

fn field_only() -> RepoInput {
    RepoInput {
        active: true,
        buf: "/repos/current".to_string(),
        prefilled: true,
        candidates: Vec::new(),
        picker: None,
    }
}

#[test]
fn the_browser_fills_the_body_with_the_directories_it_read() {
    let (_guard, repo_input) = browsing(&["alpha", "zeta"]);

    let screen = drawn_empty(&repo_input, None, false);

    assert!(
        screen.contains("browse"),
        "the box names where it is:\n{screen}"
    );
    assert!(
        screen.contains("alpha") && screen.contains("zeta"),
        "{screen}"
    );
}

#[test]
fn an_empty_directory_says_so_rather_than_drawing_a_blank_box() {
    let (_guard, repo_input) = browsing(&[]);

    let screen = drawn_empty(&repo_input, None, false);

    assert!(screen.contains("no sub-directories"), "{screen}");
}

#[test]
fn the_dialog_advertises_its_keys_on_the_input_row() {
    // Nothing else can: the dialog replaces the hint legend entirely, so an
    // unadvertised key is unfindable.
    let field = field_only();
    let line = repo_input_line(&field, Color::Yellow, 90).to_string();

    assert!(line.contains("/repos/current"), "the path itself: {line}");
    assert!(
        line.contains("↓: browse"),
        "the way into the browser: {line}"
    );
    assert!(line.contains("tab: complete"), "{line}");
}

#[test]
fn the_browsers_own_keys_replace_the_fields_on_the_input_row() {
    let (_guard, repo_input) = browsing(&["alpha"]);

    let line = repo_input_line(&repo_input, Color::Yellow, 200).to_string();

    assert!(line.contains("enter: select"), "not `enter: open`: {line}");
    assert!(line.contains("←: up"), "{line}");
    assert!(!line.contains("↓: browse"), "already browsing: {line}");
}

#[test]
fn a_path_too_long_for_the_legend_drops_it_whole_and_keeps_the_caret() {
    let mut field = field_only();
    field.buf = "/a".repeat(30);

    let line = repo_input_line(&field, Color::Yellow, 40).to_string();

    assert!(
        !line.contains("browse"),
        "a half legend reads as a glitch: {line}"
    );
    assert!(line.ends_with('█'), "the caret has to survive: {line}");
}
