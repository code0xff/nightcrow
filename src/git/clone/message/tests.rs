use super::*;

/// What a GitHub remote answers over ssh for a repository that is missing or
/// that the key cannot see. The advice block is why the last line is useless.
const REPOSITORY_NOT_FOUND: &str = "\
ERROR: Repository not found.
fatal: Could not read from remote repository.

Please make sure you have the correct access rights
and the repository exists.";

#[test]
fn the_advice_block_does_not_become_the_message() {
    let message = actionable(REPOSITORY_NOT_FOUND).expect("a message");

    assert!(
        message.starts_with("ERROR: Repository not found."),
        "the cause must lead: {message}"
    );
    assert!(
        !message.contains("and the repository exists"),
        "advice must not survive: {message}"
    );
}

#[test]
fn a_wrapped_fatal_keeps_what_the_transport_said() {
    let stderr = "\
git@github.com: Permission denied (publickey).
fatal: Could not read from remote repository.

Please make sure you have the correct access rights
and the repository exists.";

    let message = actionable(stderr).expect("a message");

    assert_eq!(
        message,
        "git@github.com: Permission denied (publickey). \
         fatal: Could not read from remote repository."
    );
}

#[test]
fn an_https_failure_keeps_both_diagnostics() {
    let stderr = "\
remote: Repository not found.
fatal: repository 'https://example.com/team/thing.git/' not found";

    let message = actionable(stderr).expect("a message");

    assert!(message.starts_with("remote: Repository not found."));
    assert!(message.ends_with("not found"));
}

#[test]
fn a_lone_diagnostic_is_the_whole_message() {
    let message = actionable("fatal: destination path 'x' already exists").expect("a message");

    assert_eq!(message, "fatal: destination path 'x' already exists");
}

#[test]
fn progress_before_a_diagnostic_is_not_carried_along() {
    // Progress arrives as one carriage-return ribbon, so it reads as a single
    // very long line — the reason a supporting line has a length bound.
    let ribbon = "remote: Counting objects: ".repeat(40);
    let stderr = format!("{ribbon}\nfatal: early EOF");

    let message = actionable(&stderr).expect("a message");

    assert_eq!(message, "fatal: early EOF");
}

#[test]
fn stderr_with_no_diagnostic_prefix_falls_back_to_its_last_line() {
    let stderr = "ssh: Could not resolve hostname nope: nodename nor servname provided\n";

    let message = actionable(stderr).expect("a message");

    assert_eq!(
        message,
        "ssh: Could not resolve hostname nope: nodename nor servname provided"
    );
}

#[test]
fn empty_stderr_yields_no_message() {
    assert_eq!(actionable(""), None);
    assert_eq!(actionable("\n  \n"), None);
}

#[test]
fn stderr_is_kept_to_its_tail() {
    // A remote controls this stream through `remote:` sidebands, so it must
    // not be collected unbounded — and the tail is the part that matters.
    let noise = "x".repeat(MAX_STDERR_BYTES * 2);
    let wanted = "\nfatal: repository not found";
    let input = format!("{noise}{wanted}");

    let kept = tail_of(input.as_bytes());

    assert!(kept.len() <= MAX_STDERR_BYTES, "kept {} bytes", kept.len());
    assert!(
        kept.ends_with("fatal: repository not found"),
        "the last line must survive the cap"
    );
}

#[test]
fn stderr_shorter_than_the_cap_is_kept_whole() {
    assert_eq!(tail_of(b"fatal: nope".as_slice()), "fatal: nope");
}
