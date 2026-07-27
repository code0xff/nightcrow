use super::*;

#[test]
fn https_url_yields_the_repository_name() {
    assert_eq!(
        validate_clone_url("https://github.com/code0xff/nightcrow.git"),
        Ok("nightcrow".to_string())
    );
}

#[test]
fn a_missing_git_suffix_is_fine() {
    assert_eq!(
        validate_clone_url("https://github.com/code0xff/nightcrow"),
        Ok("nightcrow".to_string())
    );
}

#[test]
fn scp_like_remotes_are_accepted() {
    assert_eq!(
        validate_clone_url("git@github.com:code0xff/nightcrow.git"),
        Ok("nightcrow".to_string())
    );
    assert_eq!(
        validate_clone_url("github.com:code0xff/nightcrow"),
        Ok("nightcrow".to_string())
    );
}

#[test]
fn the_ssh_schemes_are_accepted() {
    assert_eq!(
        validate_clone_url("ssh://git@github.com/code0xff/nightcrow.git"),
        Ok("nightcrow".to_string())
    );
    assert_eq!(
        validate_clone_url("git+ssh://git@example.com/team/thing.git"),
        Ok("thing".to_string())
    );
}

#[test]
fn the_anonymous_git_protocol_is_rejected() {
    // No authentication, no encryption, and no stall control — `https://`
    // serves the same anonymous fetch without any of the three.
    assert_eq!(
        validate_clone_url("git://example.com/thing.git"),
        Err(CloneUrlError::Scheme)
    );
}

#[test]
fn a_trailing_slash_does_not_swallow_the_name() {
    assert_eq!(
        validate_clone_url("https://example.com/team/thing.git/"),
        Ok("thing".to_string())
    );
}

#[test]
fn a_query_or_fragment_is_not_part_of_the_name() {
    assert_eq!(
        validate_clone_url("https://example.com/team/thing.git?ref=main"),
        Ok("thing".to_string())
    );
    assert_eq!(
        validate_clone_url("https://example.com/team/thing#frag"),
        Ok("thing".to_string())
    );
}

#[test]
fn the_ext_transport_is_rejected() {
    // git runs `ext::<command>` as a command, so accepting this scheme would
    // be remote code execution regardless of how the URL reaches git.
    assert_eq!(
        validate_clone_url("ext::sh -c whoami"),
        Err(CloneUrlError::Scheme)
    );
}

#[test]
fn other_transport_helpers_are_rejected() {
    for url in [
        "file:///etc",
        "/etc/passwd",
        "./relative/path",
        "transport::thing",
    ] {
        assert_eq!(
            validate_clone_url(url),
            Err(CloneUrlError::Scheme),
            "must reject {url}"
        );
    }
}

#[test]
fn control_characters_are_rejected() {
    assert_eq!(
        validate_clone_url("https://example.com/a\nb.git"),
        Err(CloneUrlError::Control)
    );
    assert_eq!(
        validate_clone_url("https://example.com/a\0b.git"),
        Err(CloneUrlError::Control)
    );
}

#[test]
fn an_empty_or_blank_url_is_rejected() {
    assert_eq!(validate_clone_url(""), Err(CloneUrlError::Empty));
    assert_eq!(validate_clone_url("   "), Err(CloneUrlError::Empty));
}

#[test]
fn an_over_long_url_is_rejected() {
    let url = format!("https://example.com/{}", "a".repeat(MAX_CLONE_URL_BYTES));

    assert_eq!(validate_clone_url(&url), Err(CloneUrlError::TooLong));
}

#[test]
fn a_scheme_with_no_body_is_rejected() {
    assert_eq!(validate_clone_url("https://"), Err(CloneUrlError::Scheme));
}

#[test]
fn a_url_with_no_usable_name_is_rejected() {
    // Host only: nothing after the scheme names a repository.
    assert_eq!(
        validate_clone_url("https://example.com/"),
        Err(CloneUrlError::NoName)
    );
    // A dot-name would create a hidden directory, which the picker never
    // lists — the same rule `mkdir` applies to a typed folder name.
    assert_eq!(
        validate_clone_url("https://example.com/team/.git"),
        Err(CloneUrlError::NoName)
    );
}

#[test]
fn every_error_carries_a_message() {
    for err in [
        CloneUrlError::Empty,
        CloneUrlError::TooLong,
        CloneUrlError::Control,
        CloneUrlError::Scheme,
        CloneUrlError::NoName,
    ] {
        assert!(!err.message().is_empty());
    }
}

#[test]
fn cloning_a_local_repository_produces_a_working_copy() {
    let (_src, src_path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let dest = dir.path().join("copy");

    // A local path is not a valid *URL* for the form, but `run_clone` itself
    // takes whatever the caller validated, so the process wiring is testable
    // without reaching the network.
    run_clone(&src_path, &dest).expect("clone a local repository");

    assert!(dest.join(".git").exists());
}

#[test]
fn a_failing_clone_reports_gits_own_message() {
    let dir = tempfile::TempDir::new().unwrap();
    let dest = dir.path().join("copy");
    let missing = dir.path().join("not-a-repo");

    let err = run_clone(missing.to_str().unwrap(), &dest).expect_err("must fail");

    assert!(!err.to_string().is_empty());
    assert!(!dest.exists() || std::fs::read_dir(&dest).unwrap().next().is_none());
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
