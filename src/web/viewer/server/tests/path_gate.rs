//! What each route will accept as a `path`.
//!
//! Two gates, and which one a route takes is the whole of it: a path handed to
//! git is validated as a pathspec, and a path this process is about to open is
//! also resolved on disk and refused if it is a symlink. Getting that backwards
//! has gone both ways here — `/api/diff` once accepted `../../etc/passwd`, and
//! later refused a file someone had merely deleted.

use super::{body_of, get, run_git, seeded_server};

#[test]
fn a_traversal_path_is_refused_by_every_route_that_takes_one() {
    let (dir, server, token, id) = seeded_server();

    for route in ["tree", "file", "diff"] {
        // The last two are the same two attacks in the spellings a filesystem
        // resolves for you: Windows drops a component's trailing spaces, and
        // NTFS gives `.git` an 8.3 short name.
        for attack in [
            "../../etc/passwd",
            ".git/config",
            "src/../.git/config",
            ".. /.. /etc/passwd",
            "GIT~1/config",
        ] {
            let encoded = attack.replace('/', "%2F");
            let response = get(
                server.addr(),
                &format!("/api/{route}?repo={id}&path={encoded}"),
                Some(&token),
            );
            assert!(
                response.starts_with("HTTP/1.1 400"),
                "{route} accepted {attack:?}: {response}"
            );
        }
    }
    drop(dir);
}

#[test]
fn an_error_response_leaks_no_filesystem_detail() {
    let (dir, server, token, id) = seeded_server();

    let response = get(
        server.addr(),
        &format!("/api/file?repo={id}&path=nope.txt"),
        Some(&token),
    );

    let body = body_of(&response);
    assert!(!body.contains('/'), "a path leaked into the error: {body}");
    assert!(
        !body.contains("No such file"),
        "the io error leaked: {body}"
    );
    drop(dir);
}

/// A file the working tree no longer holds still has a diff — its deletion.
///
/// The status list shows deleted files, so this is reachable by clicking one,
/// and it answered 400: the path went through the gate that resolves it on
/// disk, which a deleted path cannot survive. The TUI has always shown it,
/// because it asks the git layer directly and never passes a gate at all.
#[test]
fn the_diff_of_a_deleted_file_is_served() {
    let (dir, server, token, id) = seeded_server();
    let repo_path = {
        let entry = server.state.session.catalog().get(&id).unwrap();
        entry.path.clone()
    };
    let doomed = std::path::Path::new(&repo_path).join("doomed.txt");
    std::fs::write(&doomed, "here for now\n").unwrap();
    run_git(&repo_path, &["add", "doomed.txt"]);
    run_git(&repo_path, &["commit", "-m", "add doomed"]);
    std::fs::remove_file(&doomed).unwrap();

    let response = get(
        server.addr(),
        &format!("/api/diff?repo={id}&path=doomed.txt"),
        Some(&token),
    );

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    assert!(
        !value["hunks"].as_array().unwrap().is_empty(),
        "a deletion is a diff, not an error: {value}"
    );
    drop(dir);
}

/// Reading a diff must not read through a symlink.
///
/// This is what lets `/api/diff` take the looser gate at all. The stricter one
/// refuses symlinks because the route behind it opens the file; git instead
/// records a symlink as a blob holding the *name* it points at, so the diff
/// discloses the link — which is the repository's own content, already visible
/// in its tree — and never what the link points to.
///
/// Unix-only because creating a symlink on Windows needs developer mode or an
/// elevated process, which a test run cannot assume. What goes unverified there
/// is git's rendering of a link, not the gate: the routes are the same, and the
/// deleted-file tests above cover them on every platform.
#[test]
#[cfg(unix)]
fn a_diff_through_a_symlink_shows_the_link_not_its_target() {
    let (dir, server, token, id) = seeded_server();
    let repo_path = {
        let entry = server.state.session.catalog().get(&id).unwrap();
        entry.path.clone()
    };
    let secret = std::path::Path::new(&repo_path)
        .parent()
        .expect("a temporary repository has a parent")
        .join("outside-secret.txt");
    std::fs::write(&secret, "TOP SECRET CONTENTS\n").unwrap();
    std::os::unix::fs::symlink(&secret, std::path::Path::new(&repo_path).join("link.txt")).unwrap();

    let response = get(
        server.addr(),
        &format!("/api/diff?repo={id}&path=link.txt"),
        Some(&token),
    );

    // Both halves matter. Without the first, a route that answered 400 for
    // every symlink would satisfy this test while having stopped serving the
    // diffs it exists to serve.
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "a link is repository content and has a diff: {response}"
    );
    assert!(
        response.contains("outside-secret.txt"),
        "the diff of a link is the name it holds: {response}"
    );
    assert!(
        !response.contains("TOP SECRET"),
        "a symlink must not leak what it points at: {response}"
    );
    drop(dir);
}

/// The stricter gate stays on the route that opens the file.
#[test]
fn a_file_that_is_gone_is_still_refused_by_the_file_route() {
    let (dir, server, token, id) = seeded_server();

    let response = get(
        server.addr(),
        &format!("/api/file?repo={id}&path=never-existed.txt"),
        Some(&token),
    );

    assert!(!response.starts_with("HTTP/1.1 200"), "got: {response}");
    drop(dir);
}
