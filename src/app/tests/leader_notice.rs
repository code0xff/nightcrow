use super::*;

#[test]
fn leader_label_renders_ctrl_chord_as_caret_uppercase() {
    let mut app = app_with_files(vec!["a.rs"]);
    assert_eq!(app.leader_label(), "^F");
    app.leader = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL);
    assert_eq!(app.leader_label(), "^B");
}

#[test]
fn leader_label_without_ctrl_prints_raw_char() {
    let mut app = app_with_files(vec!["a.rs"]);
    app.leader = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(app.leader_label(), "x");
}

/// Expiry is keyed on the notice's kind. The previous design matched the
/// message text, which meant a kind with no matching arm — terminal, tree,
/// and session all qualified — was never cleared at all.
#[test]
fn clear_notice_only_drops_the_matching_kind() {
    let mut app = app_with_files(vec![]);

    app.raise_notice(NoticeKind::Tree, "boom");
    app.clear_notice(NoticeKind::Git);
    assert_eq!(
        app.notice,
        Some(Notice::new(NoticeKind::Tree, "boom")),
        "an unrelated subsystem's success must not drop another's notice"
    );

    app.clear_notice(NoticeKind::Tree);
    assert_eq!(app.notice, None);
}

#[test]
fn raising_a_notice_replaces_the_previous_one() {
    let mut app = app_with_files(vec![]);
    app.raise_notice(NoticeKind::Tree, "first");
    app.raise_notice(NoticeKind::Git, "second");
    assert_eq!(app.notice, Some(Notice::new(NoticeKind::Git, "second")));
}

#[test]
fn notice_line_prefixes_only_the_kinds_that_carry_a_label() {
    assert_eq!(
        Notice::new(NoticeKind::Git, "not a repo").line(),
        "git error: not a repo"
    );
    assert_eq!(
        Notice::new(NoticeKind::RepoInput, "no such directory").line(),
        "no such directory"
    );
}