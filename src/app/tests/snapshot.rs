use super::*;

#[test]
fn drain_snapshot_empties_the_queue_without_applying_it() {
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec!["old.rs"])
    };
    let send = |files: Vec<&str>| {
        SnapshotMsg::Ok(
            RepoSnapshot {
                files: files
                    .into_iter()
                    .map(|p| ChangedFile::unstaged_only(p.to_string(), StatusKind::Modified))
                    .collect(),
                tracking: None,
                head_oid: None,
                branch_name: None,
            },
            HashMap::new(),
        )
    };
    tx.send(send(vec!["first.rs"])).unwrap();
    tx.send(send(vec!["second.rs"])).unwrap();

    app.drain_snapshot();

    // The queue is empty (so a hidden project's channel cannot grow), but
    // no git work ran: the view still shows the pre-snapshot file list.
    assert!(app.snapshot.try_recv().is_err(), "queue must be drained");
    assert_eq!(app.status_view.files[0].path, "old.rs");
    assert!(app.pending_snapshot.is_some(), "the tail is held for later");

    // Applying it later yields the *last* snapshot, not the first.
    app.poll_snapshot();
    assert_eq!(app.status_view.files[0].path, "second.rs");
    assert!(app.pending_snapshot.is_none(), "pending is consumed");
}

#[test]
fn a_saved_mode_lands_immediately_and_survives_being_changed() {
    // The restore used to wait for the first snapshot and then overwrite
    // whatever the user had picked in between. Now the mode is applied on
    // the spot, so a later change is simply the newer choice.
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec![])
    };

    app.restore_session(&crate::session::SessionState {
        mode: Some(ViewMode::Tree),
        ..Default::default()
    });
    assert_eq!(app.mode, ViewMode::Tree, "applied without a snapshot");

    app.toggle_mode();
    let chosen = app.mode;
    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: Vec::new(),
            tracking: None,
            head_oid: None,
            branch_name: None,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();

    assert_eq!(app.mode, chosen, "the snapshot must not undo the choice");
}

#[test]
fn a_saved_selection_is_restored_by_the_first_snapshot() {
    // The one part that has to wait: it names a file the changed-file list
    // has not delivered yet. It rides the ordinary path-preservation code.
    let (snapshot, tx) = dummy_snapshot_channel();
    let mut app = App {
        snapshot,
        pending_snapshot: None,
        ..app_with_files(vec![])
    };

    app.restore_session(&crate::session::SessionState {
        selected_file: Some("b.rs".to_string()),
        ..Default::default()
    });
    assert!(app.pending_selection.is_some(), "held until the list lands");

    tx.send(SnapshotMsg::Ok(
        RepoSnapshot {
            files: ["a.rs", "b.rs"]
                .iter()
                .map(|p| ChangedFile::unstaged_only(p.to_string(), StatusKind::Modified))
                .collect(),
            tracking: None,
            head_oid: None,
            branch_name: None,
        },
        HashMap::new(),
    ))
    .unwrap();
    app.poll_snapshot();

    assert_eq!(app.status_view.files[app.status_view.selected].path, "b.rs");
    assert!(app.pending_selection.is_none(), "consumed");
}