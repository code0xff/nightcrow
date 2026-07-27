use super::{body_of, get, login, post, server};
use std::time::{Duration, Instant};

/// Poll the job until it leaves `running`, so the assertion sees the outcome
/// rather than the race. Bounded so a hung clone fails the test instead of
/// hanging the suite.
fn await_job(addr: std::net::SocketAddr, token: &str, job: u64) -> String {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let response = get(addr, &format!("/api/clone?job={job}"), Some(token));
        let body = body_of(&response).to_string();
        if !body.contains("\"running\"") {
            return body;
        }
        assert!(Instant::now() < deadline, "clone did not finish: {body}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn job_id(body: &str) -> u64 {
    let value: serde_json::Value = serde_json::from_str(body).expect("json body");
    value["job"].as_u64().expect("job id")
}

#[test]
fn a_started_clone_is_tracked_to_a_terminal_state() {
    // The remote is a closed local port, so `git` fails fast and the whole
    // start-poll-finish path is exercised without touching the network.
    let server = server(&[]);
    let token = login(server.addr());
    let dest_parent = tempfile::TempDir::new().unwrap();

    let started = post(
        server.addr(),
        "/api/clone",
        &serde_json::json!({
            "path": dest_parent.path(),
            "url": "https://127.0.0.1:1/team/thing.git",
        })
        .to_string(),
        Some(&token),
    );

    assert!(started.contains("200 OK"), "got: {started}");
    let body = body_of(&started);
    assert!(
        body.contains("\"thing\""),
        "must echo the name derived from the URL: {body}"
    );
    let finished = await_job(server.addr(), &token, job_id(body));
    assert!(
        finished.contains("\"failed\""),
        "an unreachable remote must land in failed with a reason: {finished}"
    );
    assert!(
        !dest_parent.path().join("thing").exists(),
        "a failed clone must not leave the destination behind"
    );
}

#[test]
fn the_ext_transport_is_refused_before_git_runs() {
    // `git clone ext::<command>` executes that command. This is the request
    // that must never reach `git`.
    let server = server(&[]);
    let token = login(server.addr());
    let dir = tempfile::TempDir::new().unwrap();
    let marker = dir.path().join("pwned");

    let response = post(
        server.addr(),
        "/api/clone",
        &serde_json::json!({
            "path": dir.path(),
            "url": format!("ext::sh -c touch{}{}", " ", marker.display()),
        })
        .to_string(),
        Some(&token),
    );

    assert!(response.contains("400 Bad Request"), "got: {response}");
    assert!(!marker.exists(), "the helper command must never run");
}

#[test]
fn a_local_path_is_refused_as_a_url() {
    let server = server(&[]);
    let token = login(server.addr());
    let dir = tempfile::TempDir::new().unwrap();

    for url in ["/etc", "file:///etc", "./thing"] {
        let response = post(
            server.addr(),
            "/api/clone",
            &serde_json::json!({ "path": dir.path(), "url": url }).to_string(),
            Some(&token),
        );
        assert!(response.contains("400 Bad Request"), "{url}: {response}");
    }
}

#[test]
fn cloning_onto_an_existing_folder_conflicts() {
    let server = server(&[]);
    let token = login(server.addr());
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(dir.path().join("nightcrow")).unwrap();

    let response = post(
        server.addr(),
        "/api/clone",
        &serde_json::json!({
            "path": dir.path(),
            "url": "https://example.com/code0xff/nightcrow.git",
        })
        .to_string(),
        Some(&token),
    );

    assert!(response.contains("409 Conflict"), "got: {response}");
}

#[test]
fn cloning_into_a_missing_directory_is_rejected() {
    let server = server(&[]);
    let token = login(server.addr());

    let response = post(
        server.addr(),
        "/api/clone",
        &serde_json::json!({
            "path": "/nonexistent/nightcrow-clone-parent",
            "url": "https://example.com/team/thing.git",
        })
        .to_string(),
        Some(&token),
    );

    assert!(response.contains("400 Bad Request"), "got: {response}");
}

#[test]
fn an_unknown_job_is_not_found() {
    let server = server(&[]);
    let token = login(server.addr());

    let response = get(server.addr(), "/api/clone?job=4242", Some(&token));

    assert!(response.contains("404 Not Found"), "got: {response}");
}

#[test]
fn a_job_id_is_required_to_poll() {
    let server = server(&[]);
    let token = login(server.addr());

    let response = get(server.addr(), "/api/clone", Some(&token));

    assert!(response.contains("400 Bad Request"), "got: {response}");
}

#[test]
fn cloning_requires_authentication() {
    let server = server(&[]);
    let dir = tempfile::TempDir::new().unwrap();

    let response = post(
        server.addr(),
        "/api/clone",
        &serde_json::json!({ "path": dir.path(), "url": "https://example.com/a/b.git" })
            .to_string(),
        None,
    );

    assert!(response.contains("401 Unauthorized"), "got: {response}");
}
