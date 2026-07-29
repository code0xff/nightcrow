use super::*;
use std::path::Path;
use tempfile::TempDir;

/// A directory holding `dirs` as sub-directories and `files` as plain files.
fn tree(dirs: &[&str], files: &[&str]) -> TempDir {
    let root = TempDir::new().expect("a temp dir");
    for d in dirs {
        std::fs::create_dir(root.path().join(d)).expect("create dir");
    }
    for f in files {
        std::fs::write(root.path().join(f), b"x").expect("create file");
    }
    root
}

/// `<root>/<frag>` as the dialog buffer would hold it. `/` is a separator on
/// every supported platform, so it stands in for whatever the user typed.
fn buf_in(root: &Path, frag: &str) -> String {
    format!("{}/{frag}", root.to_str().expect("a UTF-8 temp path"))
}

#[test]
fn completing_a_unique_directory_appends_a_separator_to_descend() {
    let root = tree(&["nightcrow"], &[]);

    let c = complete_dir_path(&buf_in(root.path(), "night"));

    assert_eq!(c.buf, buf_in(root.path(), "nightcrow/"));
    assert!(c.candidates.is_empty(), "a unique match needs no list");
}

#[test]
fn completing_an_ambiguous_prefix_extends_it_without_listing() {
    let root = tree(&["nightcrow", "nightowl"], &[]);

    let c = complete_dir_path(&buf_in(root.path(), "n"));

    assert_eq!(c.buf, buf_in(root.path(), "night"));
    assert!(
        c.candidates.is_empty(),
        "while an extension still narrows the prefix, a list is noise"
    );
}

#[test]
fn completing_a_prefix_that_cannot_grow_lists_the_candidates() {
    let root = tree(&["nightcrow", "nightowl"], &[]);
    let buf = buf_in(root.path(), "night");

    let c = complete_dir_path(&buf);

    assert_eq!(c.buf, buf, "with nothing left to extend the text stands");
    assert_eq!(c.candidates, vec!["nightcrow", "nightowl"]);
}

#[test]
fn completing_at_a_directory_boundary_lists_every_subdirectory() {
    let root = tree(&["alpha", "beta"], &[]);
    let buf = buf_in(root.path(), "");

    let c = complete_dir_path(&buf);

    assert_eq!(c.buf, buf);
    assert_eq!(c.candidates, vec!["alpha", "beta"]);
}

#[test]
fn completing_at_a_directory_boundary_lists_even_while_extending() {
    // An empty fragment means "what is in here?", so the shared `sr` prefix
    // gets applied *and* the list shown — extending alone would answer nothing.
    let root = tree(&["src", "srv"], &[]);

    let c = complete_dir_path(&buf_in(root.path(), ""));

    assert_eq!(c.buf, buf_in(root.path(), "sr"));
    assert_eq!(c.candidates, vec!["src", "srv"]);
}

#[test]
fn completing_skips_files_because_a_repo_must_be_a_directory() {
    let root = tree(&["target"], &["tags", "tsconfig.json"]);

    let c = complete_dir_path(&buf_in(root.path(), "t"));

    assert_eq!(
        c.buf,
        buf_in(root.path(), "target/"),
        "the two files must not make the directory ambiguous"
    );
}

#[test]
fn completing_hides_dotted_directories_until_the_fragment_starts_with_a_dot() {
    let root = tree(&[".config", "docs"], &[]);

    let visible = complete_dir_path(&buf_in(root.path(), ""));
    assert_eq!(visible.buf, buf_in(root.path(), "docs/"));

    let hidden = complete_dir_path(&buf_in(root.path(), "."));
    assert_eq!(hidden.buf, buf_in(root.path(), ".config/"));
}

#[test]
fn completing_an_unmatched_prefix_leaves_the_buffer_alone() {
    let root = tree(&["docs"], &[]);
    let buf = buf_in(root.path(), "zzz");

    let c = complete_dir_path(&buf);

    assert_eq!(c.buf, buf);
    assert!(c.candidates.is_empty());
}

#[test]
fn completing_an_empty_directory_leaves_the_buffer_alone() {
    let root = tree(&[], &["only-a-file"]);
    let buf = buf_in(root.path(), "");

    let c = complete_dir_path(&buf);

    assert_eq!(c.buf, buf);
    assert!(c.candidates.is_empty());
}

#[test]
fn completing_inside_an_unreadable_directory_leaves_the_buffer_alone() {
    // Mid-typing the buffer routinely names a directory that does not exist.
    // That is not an error worth reporting, so it must be a silent no-op.
    let buf = "/nonexistent-dir-for-nightcrow-tests/x".to_string();

    let c = complete_dir_path(&buf);

    assert_eq!(c.buf, buf);
    assert!(c.candidates.is_empty());
}

#[test]
fn completing_a_bare_fragment_with_no_separator_does_not_panic() {
    // Exercises the cwd branch without depending on the working directory's
    // contents, which other tests may be changing in parallel.
    let buf = "zzz-no-such-prefix-in-cwd".to_string();

    let c = complete_dir_path(&buf);

    assert_eq!(c.buf, buf);
    assert!(c.candidates.is_empty());
}

#[test]
fn completing_keeps_a_tilde_literal_instead_of_expanding_it_into_the_buffer() {
    // `~` is expanded to read the directory, but writing the expansion back
    // would replace the user's text with an absolute path they never typed.
    let home = dirs::home_dir().expect("a home directory");
    let home_str = home.to_str().expect("a UTF-8 home path");

    let c = complete_dir_path("~/");

    assert!(
        c.buf.starts_with("~/"),
        "the typed prefix must survive, got: {}",
        c.buf
    );
    assert!(
        !c.buf.contains(home_str),
        "the home path must not be written into the buffer, got: {}",
        c.buf
    );
}

#[test]
fn completing_falls_back_to_ignoring_case_and_corrects_the_typed_casing() {
    let root = tree(&["Documents"], &[]);

    let c = complete_dir_path(&buf_in(root.path(), "docu"));

    assert_eq!(
        c.buf,
        buf_in(root.path(), "Documents/"),
        "a case-insensitive match must be recased to the name on disk"
    );
}

// Only a case-sensitive filesystem can hold `Docs` and `docs` side by side;
// APFS and NTFS reject the second `create_dir` as AlreadyExists. The fallback
// this covers therefore only has anything to disambiguate on Linux.
#[cfg(target_os = "linux")]
#[test]
fn completing_prefers_an_exact_case_match_over_a_case_insensitive_one() {
    let root = tree(&["Docs", "docs"], &[]);

    let c = complete_dir_path(&buf_in(root.path(), "docs"));

    assert_eq!(
        c.buf,
        buf_in(root.path(), "docs/"),
        "the exact-case pass must resolve this before the fallback runs"
    );
}

#[test]
fn completing_handles_multi_byte_names_on_a_char_boundary() {
    let root = tree(&["한국어-프로젝트", "한국어-문서"], &[]);

    let c = complete_dir_path(&buf_in(root.path(), "한"));

    assert_eq!(
        c.buf,
        buf_in(root.path(), "한국어-"),
        "the shared prefix must stop on a char boundary, not a byte one"
    );
}

// APFS validates filenames as UTF-8 and rejects the create outright, so only
// Linux can stage an entry the listing has to skip.
#[cfg(target_os = "linux")]
#[test]
fn completing_skips_names_that_are_not_valid_utf8() {
    use std::os::unix::ffi::OsStrExt;

    let root = TempDir::new().expect("a temp dir");
    std::fs::create_dir(root.path().join("docs")).expect("create dir");
    let invalid = std::ffi::OsStr::from_bytes(b"do\xffcs");
    std::fs::create_dir(root.path().join(invalid)).expect("create dir");

    let c = complete_dir_path(&buf_in(root.path(), "do"));

    assert_eq!(
        c.buf,
        buf_in(root.path(), "docs/"),
        "a non-UTF-8 name cannot round-trip the buffer, so it must not compete"
    );
}

#[cfg(windows)]
#[test]
fn completing_reuses_the_separator_already_in_the_buffer() {
    let root = tree(&["docs"], &[]);
    let base = root.path().to_str().expect("a UTF-8 temp path").to_string();

    let c = complete_dir_path(&format!("{base}/do"));

    assert!(
        c.buf.ends_with("docs/"),
        "a buffer typed with `/` must not gain a `\\`, got: {}",
        c.buf
    );
}

#[cfg(unix)]
#[test]
fn completing_follows_a_symlink_to_a_directory() {
    // Symlinked checkouts are common, and reporting one as a non-directory
    // would hide a real repo from the picker.
    let root = tree(&["real"], &[]);
    std::os::unix::fs::symlink(root.path().join("real"), root.path().join("linked"))
        .expect("create symlink");

    let c = complete_dir_path(&buf_in(root.path(), "link"));

    assert_eq!(c.buf, buf_in(root.path(), "linked/"));
}
