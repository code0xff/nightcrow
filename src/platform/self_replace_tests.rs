use super::*;

fn bin(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"binary").unwrap();
    path
}

#[test]
fn vacating_frees_the_install_path() {
    let dir = tempfile::tempdir().unwrap();
    let exe = bin(dir.path(), "nightcrow.exe");

    let parked = vacate(&exe).unwrap().expect("an existing binary is parked");

    assert!(!exe.exists(), "the install path is free for the installer");
    assert_eq!(std::fs::read(&parked).unwrap(), b"binary");
}

#[test]
fn vacating_a_missing_binary_is_not_an_error() {
    let dir = tempfile::tempdir().unwrap();

    assert!(vacate(&dir.path().join("nightcrow.exe")).unwrap().is_none());
}

#[test]
fn restoring_undoes_a_vacate() {
    let dir = tempfile::tempdir().unwrap();
    let exe = bin(dir.path(), "nightcrow.exe");
    let parked = vacate(&exe).unwrap().unwrap();

    restore(&parked, &exe).unwrap();

    assert_eq!(std::fs::read(&exe).unwrap(), b"binary");
    assert!(!parked.exists());
}

#[test]
fn a_second_vacate_reuses_the_slot_of_a_deletable_leftover() {
    let dir = tempfile::tempdir().unwrap();
    let exe = bin(dir.path(), "nightcrow.exe");
    let first = vacate(&exe).unwrap().unwrap();
    bin(dir.path(), "nightcrow.exe");

    let second = vacate(&exe).unwrap().unwrap();

    assert_eq!(first, second, "the freed slot is reused, not accumulated");
}

#[test]
fn sweeping_removes_parked_binaries_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let exe = bin(dir.path(), "nightcrow.exe");
    let parked = vacate(&exe).unwrap().unwrap();
    let bystander = bin(dir.path(), "nightcrow.exe.nightcrow-old-notes");
    let other = bin(dir.path(), "some-other-tool.exe");

    sweep(&exe);

    assert!(!parked.exists());
    assert!(
        bystander.exists(),
        "a name without a slot number is not ours"
    );
    assert!(other.exists());
}

#[test]
fn parked_names_are_recognised_only_with_a_numeric_slot() {
    assert!(is_parked_name("nightcrow.exe.nightcrow-old.0"));
    assert!(is_parked_name("nightcrow.nightcrow-old.17"));
    assert!(!is_parked_name("nightcrow.exe.nightcrow-old"));
    assert!(!is_parked_name("nightcrow.exe.nightcrow-old.keep"));
    assert!(!is_parked_name("nightcrow.exe"));
}
