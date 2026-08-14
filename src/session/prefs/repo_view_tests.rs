//! Remembering what each project was last showing in the browser.

use super::*;

fn view(repo: &str) -> RepoView {
    RepoView {
        repo: repo.to_string(),
        tab: ViewTab::Status,
        file: None,
        tree_expanded: Vec::new(),
    }
}

fn with_file(repo: &str, path: &str) -> RepoView {
    RepoView {
        file: Some(ViewFile {
            path: path.to_string(),
            commit: None,
            face: ViewFace::Diff,
        }),
        ..view(repo)
    }
}

#[test]
fn a_project_gets_back_what_it_was_showing() {
    let mut list = Vec::new();
    remember(&mut list, with_file("/a", "src/main.rs"));
    remember(&mut list, with_file("/b", "README.md"));

    assert_eq!(
        view_of(&list, "/a")
            .and_then(|v| v.file.as_ref())
            .map(|f| f.path.as_str()),
        Some("src/main.rs")
    );
    assert_eq!(
        view_of(&list, "/b")
            .and_then(|v| v.file.as_ref())
            .map(|f| f.path.as_str()),
        Some("README.md")
    );
    assert!(view_of(&list, "/never-opened").is_none());
}

#[test]
fn looking_at_a_project_again_replaces_rather_than_stacks() {
    let mut list = Vec::new();
    remember(&mut list, with_file("/a", "one.rs"));
    remember(&mut list, with_file("/a", "two.rs"));

    assert_eq!(list.len(), 1);
    assert_eq!(
        view_of(&list, "/a").unwrap().file.as_ref().unwrap().path,
        "two.rs"
    );
}

#[test]
fn the_oldest_views_go_once_the_cap_is_reached() {
    let mut list = Vec::new();
    for i in 0..MAX_REMEMBERED_VIEWS + 5 {
        remember(&mut list, view(&format!("/repo{i}")));
    }

    assert_eq!(list.len(), MAX_REMEMBERED_VIEWS);
    assert!(
        view_of(&list, "/repo0").is_none(),
        "the first opened is the first to go"
    );
    assert!(view_of(&list, "/repo4").is_none());
    assert!(view_of(&list, "/repo5").is_some());
}

/// The file is read back into a request to open it, so a path reaching outside
/// the project cannot be stored — whether it came from a client or from someone
/// editing `viewer.json`.
#[test]
fn a_path_leaving_the_project_is_dropped_rather_than_stored() {
    let mut list = Vec::new();
    for escape in ["../secrets.txt", "/etc/passwd", ""] {
        remember(&mut list, with_file("/a", escape));
        assert!(
            view_of(&list, "/a").unwrap().file.is_none(),
            "must not store {escape:?}"
        );
    }
}

#[test]
fn a_commit_id_that_is_not_one_takes_the_file_with_it() {
    let mut list = Vec::new();
    remember(
        &mut list,
        RepoView {
            file: Some(ViewFile {
                path: "src/main.rs".to_string(),
                commit: Some("; rm -rf /".to_string()),
                face: ViewFace::Diff,
            }),
            ..view("/a")
        },
    );

    assert!(view_of(&list, "/a").unwrap().file.is_none());
}

#[test]
fn a_real_commit_id_is_kept() {
    let mut list = Vec::new();
    remember(
        &mut list,
        RepoView {
            file: Some(ViewFile {
                path: "src/main.rs".to_string(),
                commit: Some("9f1c0d2".to_string()),
                face: ViewFace::Source,
            }),
            ..view("/a")
        },
    );

    let file = view_of(&list, "/a").unwrap().file.as_ref().unwrap();
    assert_eq!(file.commit.as_deref(), Some("9f1c0d2"));
    assert_eq!(file.face, ViewFace::Source);
}

#[test]
fn the_expanded_tree_is_filtered_and_capped() {
    let mut list = Vec::new();
    let mut expanded: Vec<String> = (0..MAX_TREE_EXPANDED + 10)
        .map(|i| format!("dir{i}"))
        .collect();
    expanded.push("../elsewhere".to_string());
    remember(
        &mut list,
        RepoView {
            tab: ViewTab::Tree,
            tree_expanded: expanded,
            ..view("/a")
        },
    );

    let stored = view_of(&list, "/a").unwrap();
    assert_eq!(stored.tree_expanded.len(), MAX_TREE_EXPANDED);
    assert!(!stored.tree_expanded.iter().any(|p| p.contains("..")));
}

/// A file this build did not write: duplicated projects, an oversized list, and
/// paths no write would have accepted.
#[test]
fn a_hand_edited_file_is_held_to_what_a_write_would_have_produced() {
    let mut list = vec![
        with_file("/a", "kept.rs"),
        with_file("/a", "shadowed.rs"),
        with_file("/b", "../escape.rs"),
    ];

    normalize(&mut list);

    assert_eq!(list.len(), 2, "the duplicate goes");
    assert_eq!(
        view_of(&list, "/a").unwrap().file.as_ref().unwrap().path,
        "kept.rs"
    );
    assert!(
        view_of(&list, "/b").unwrap().file.is_none(),
        "the escaping path goes, the project's entry stays"
    );
}

/// The file is the whole point: a view is worth keeping only if it outlives the
/// process that stored it. Also proves the load path holds a written file to the
/// same rules — the tree here is capped on the way in and stays capped.
#[test]
fn a_view_round_trips_through_the_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nested").join("viewer.json");

    crate::session::prefs::PrefsStore::at(path.clone()).set_view(RepoView {
        tab: ViewTab::Tree,
        tree_expanded: vec!["src".to_string(), "src/ui".to_string()],
        ..with_file("/a", "src/ui/mod.rs")
    });

    let reloaded = crate::session::prefs::PrefsStore::at(path).get();
    let stored = view_of(&reloaded.views, "/a").expect("the project is remembered");
    assert_eq!(stored.tab, ViewTab::Tree);
    assert_eq!(stored.tree_expanded, ["src", "src/ui"]);
    assert_eq!(stored.file.as_ref().unwrap().path, "src/ui/mod.rs");
}

#[test]
fn tabs_and_faces_round_trip_through_their_wire_names() {
    for tab in [ViewTab::Status, ViewTab::Log, ViewTab::Tree] {
        assert_eq!(ViewTab::parse(tab.as_str()), Some(tab));
    }
    for face in [ViewFace::Diff, ViewFace::Source] {
        assert_eq!(ViewFace::parse(face.as_str()), Some(face));
    }
    assert_eq!(ViewTab::parse("diff"), None);
    assert_eq!(ViewTab::parse(""), None);
    assert_eq!(ViewFace::parse("rendered"), None);
}
