use super::*;

#[test]
fn a_file_written_in_the_work_tree_is_read_at_once() {
    let (dir, path) = make_repo();
    let channel = watched(&path);

    std::fs::write(Path::new(&path).join("edited.rs"), "fn main() {}").unwrap();

    let SnapshotMsg::Ok(snapshot, _) = next_read(&channel) else {
        panic!("the read failed");
    };
    assert!(snapshot.files.iter().any(|file| file.path == "edited.rs"));
    drop(dir);
}

#[test]
fn staging_a_file_is_read_even_though_the_work_tree_did_not_change() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join("staged.rs"), "fn main() {}").unwrap();
    let channel = watched(&path);
    reads_during(&channel, Duration::from_millis(300));

    run_git(&path, &["add", "staged.rs"]);

    let SnapshotMsg::Ok(snapshot, _) = next_read(&channel) else {
        panic!("the read failed");
    };
    let staged = snapshot
        .files
        .iter()
        .find(|file| file.path == "staged.rs")
        .expect("the file is listed");
    assert_eq!(staged.short_code(), "A ");
    drop(dir);
}

#[test]
fn staging_in_a_linked_worktree_is_read_too() {
    let (main, elsewhere, tree) = make_linked_worktree();
    std::fs::write(Path::new(&tree).join("staged.rs"), "fn main() {}").unwrap();
    let channel = watched(&tree);
    quiesce(&channel);

    run_git(&tree, &["add", "staged.rs"]);

    let SnapshotMsg::Ok(snapshot, _) = next_read(&channel) else {
        panic!("the read failed");
    };
    let staged = snapshot
        .files
        .iter()
        .find(|file| file.path == "staged.rs")
        .expect("the file is listed");
    assert_eq!(staged.short_code(), "A ");
    drop((main, elsewhere));
}

#[test]
fn a_burst_of_changes_is_read_at_the_old_pace_rather_than_per_event() {
    let (dir, path) = make_repo();
    let channel = watched(&path);

    for i in 0..400 {
        std::fs::write(Path::new(&path).join(format!("f{i}.rs")), "fn main() {}").unwrap();
    }
    let window = Duration::from_millis(2_500);
    let reads = reads_during(&channel, window);

    let ceiling = (window.as_millis() / MIN_READ_INTERVAL.as_millis()) as usize + 1;
    assert!(reads <= ceiling, "{reads} reads; ceiling is {ceiling}");
    assert!(reads >= 1, "the changes must be read");
    drop(dir);
}

#[test]
fn changes_git_ignores_do_not_cause_a_read() {
    let (dir, path) = make_repo();
    std::fs::write(Path::new(&path).join(".gitignore"), "out/\n").unwrap();
    std::fs::create_dir_all(Path::new(&path).join("out")).unwrap();
    let channel = watched(&path);
    quiesce(&channel);

    for i in 0..200 {
        std::fs::write(Path::new(&path).join(format!("out/artifact{i}.o")), "x").unwrap();
    }

    assert_eq!(reads_during(&channel, SETTLE), 0);
    drop(dir);
}
