use super::*;

#[test]
fn app_facade_adopts_repository_identity_without_replacing_view_state() {
    let mut app = app_with_files(vec!["src/lib.rs"]);
    app.git.view.status.selected = 0;

    app.adopt_repository_id("opaque-id".to_string());

    assert_eq!(app.repository_id(), Some("opaque-id"));
    assert_eq!(app.repository_path(), ".");
    assert_eq!(app.status_view().selected, 0);
    assert_eq!(app.status_view().files[0].path, "src/lib.rs");
}
