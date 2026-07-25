mod auth;
mod commit_routes;
mod prefs;
mod reorder;
mod routes;
mod terminals;

use super::{VIEWER_SESSION_COOKIE, ViewerOptions, ViewerServer};
use crate::test_util::{make_repo, run_git};
use crate::web::common::auth::Auth;
use crate::web::viewer::prefs::PrefsStore;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

pub(super) fn server(paths: &[String]) -> ViewerServer {
    server_with(paths, crate::config::AgentIndicatorConfig::default(), None)
}

/// `prefs_dir` keeps preference writes inside a temp directory. Left `None`
/// the store still points at the real `~/.nightcrow/viewer.json`, so any
/// test that *writes* a preference must pass one.
pub(super) fn server_with(
    paths: &[String],
    hot: crate::config::AgentIndicatorConfig,
    prefs_dir: Option<&std::path::Path>,
) -> ViewerServer {
    let prefs = match prefs_dir {
        Some(dir) => PrefsStore::at(dir.join("viewer.json")),
        None => PrefsStore::at(std::path::PathBuf::from(
            "/nonexistent/nightcrow/viewer.json",
        )),
    };
    ViewerServer::start(ViewerOptions {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        auth: Auth::from_plaintext("swordfish").unwrap(),
        repos: paths.to_vec(),
        // Never persist from tests — they must not touch the real
        // ~/.nightcrow/workspace.json.
        persist: false,
        startup_commands: Vec::new(),
        hot,
        prefs,
    })
    .unwrap()
}

/// Send a raw request and return the full response text.
pub(super) fn request(addr: SocketAddr, raw: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut buf = Vec::new();
    let _ = stream.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

pub(super) fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> String {
    let cookie = match cookie {
        Some(token) => format!("Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n"),
        None => String::new(),
    };
    request(
        addr,
        &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{cookie}Connection: close\r\n\r\n"),
    )
}

pub(super) fn post(addr: SocketAddr, path: &str, body: &str, cookie: Option<&str>) -> String {
    let cookie = match cookie {
        Some(token) => format!("Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n"),
        None => String::new(),
    };
    request(
        addr,
        &format!(
            "POST {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
             Content-Type: application/json\r\n{cookie}\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
}

pub(super) fn delete(addr: SocketAddr, path: &str, cookie: Option<&str>) -> String {
    let cookie = match cookie {
        Some(token) => format!("Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n"),
        None => String::new(),
    };
    request(
        addr,
        &format!("DELETE {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{cookie}Connection: close\r\n\r\n"),
    )
}

/// Log in and return the session token.
pub(super) fn login(addr: SocketAddr) -> String {
    let body = "password=swordfish";
    let response = request(
        addr,
        &format!(
            "POST /login HTTP/1.1\r\nHost: 127.0.0.1\r\n\
             Content-Type: application/x-www-form-urlencoded\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(
        response.starts_with("HTTP/1.1 200"),
        "login failed: {response}"
    );
    response
        .split("Set-Cookie: ")
        .nth(1)
        .and_then(|rest| rest.split(';').next())
        .and_then(|pair| pair.split_once('=').map(|(_, v)| v.to_string()))
        .expect("a session cookie")
}

pub(super) fn body_of(response: &str) -> &str {
    response.split("\r\n\r\n").nth(1).unwrap_or("")
}

pub(super) fn seeded_server() -> (tempfile::TempDir, ViewerServer, String, String) {
    let (dir, path) = make_repo();
    std::fs::create_dir(std::path::Path::new(&path).join("src")).unwrap();
    std::fs::write(
        std::path::Path::new(&path).join("src/main.rs"),
        "fn main() {}\n",
    )
    .unwrap();
    run_git(&path, &["add", "."]);
    run_git(&path, &["commit", "-m", "init"]);

    let server = server(std::slice::from_ref(&path));
    let token = login(server.addr());
    let listing = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&listing)).unwrap();
    let id = value["repos"][0]["id"].as_str().unwrap().to_string();
    (dir, server, token, id)
}

/// A repository whose history is one commit longer than a page, so the
/// first page is full and a second one exists.
pub(super) fn server_with_paged_history()
-> (tempfile::TempDir, ViewerServer, String, String, String) {
    let (dir, path) = make_repo();
    for i in 0..=crate::web::viewer::limits::MAX_LOG_PAGE {
        std::fs::write(std::path::Path::new(&path).join(format!("f{i}")), "x").unwrap();
        run_git(&path, &["add", "."]);
        run_git(&path, &["commit", "-m", &format!("c{i}")]);
    }
    let server = server(std::slice::from_ref(&path));
    let token = login(server.addr());
    let listing = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&listing)).unwrap();
    let id = value["repos"][0]["id"].as_str().unwrap().to_string();
    (dir, server, token, id, path)
}

pub(super) fn log_page(server: &ViewerServer, token: &str, query: &str) -> serde_json::Value {
    let response = get(server.addr(), &format!("/api/log?{query}"), Some(token));
    serde_json::from_str(body_of(&response)).unwrap()
}

#[test]
fn the_app_shell_is_reachable_without_a_session() {
    // The bundle renders the login form, so gating it would leave the user
    // with no way to authenticate at all.
    let (dir, path) = make_repo();
    let server = server(&[path]);

    let response = get(server.addr(), "/", None);

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert!(response.contains("<div id=\"root\">"), "not the app shell");
    assert!(
        response.contains("Content-Security-Policy"),
        "the shell must carry a CSP"
    );
    drop(dir);
}

#[test]
fn an_empty_catalog_serves_cleanly() {
    // The TUI can start with no project open, so the viewer alongside it
    // sees an empty catalog. That is a legitimate state, not an error.
    let server = server(&[]);
    let token = login(server.addr());

    let response = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
    assert_eq!(value["repos"].as_array().unwrap().len(), 0);
}

#[test]
fn the_repository_list_serves_the_configured_recently_touched_settings() {
    // The client fades its file list on this window; reading the config
    // from the server is what keeps it on the TUI's window rather than a
    // second default that drifts.
    let server = server_with(
        &[],
        crate::config::AgentIndicatorConfig {
            enabled: false,
            hot_window_secs: 42,
            auto_follow: true,
        },
        None,
    );
    let token = login(server.addr());

    let response = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert_eq!(value["hot"]["enabled"], false);
    assert_eq!(value["hot"]["window_secs"], 42);
    // `auto_follow` moves a TUI selection; the browser has no analogue and
    // must not be told about it.
    assert!(value["hot"].get("auto_follow").is_none());
}

#[test]
fn the_repository_list_serves_the_server_clock_for_dating_mtimes() {
    // `mtime` is an absolute instant on this machine's clock, so a browser
    // on a device whose clock disagrees needs the reference to subtract.
    use crate::web::viewer::dto::server_now_millis;

    let server = server_with(&[], crate::config::AgentIndicatorConfig::default(), None);
    let token = login(server.addr());
    let before = server_now_millis();

    let response = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    let now_ms = value["now_ms"].as_u64().expect("now_ms is a number");
    let after = server_now_millis();
    assert!(
        (before..=after).contains(&now_ms),
        "now_ms {now_ms} outside the request window {before}..={after}",
    );
}

#[test]
fn repos_lists_the_served_set_by_opaque_id() {
    let (dir, path) = make_repo();
    let server = server(std::slice::from_ref(&path));
    let token = login(server.addr());

    let response = get(server.addr(), "/api/repos", Some(&token));
    let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

    assert_eq!(value["version"], crate::web::viewer::dto::PROTOCOL_VERSION);
    let repo = &value["repos"][0];
    assert!(repo["id"].as_str().unwrap().starts_with('r'));
    let mut keys: Vec<_> = repo.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["display_path", "id", "name"],
        "only the whitelisted identity fields may be listed"
    );
    drop((dir, path));
}
