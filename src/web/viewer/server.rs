//! The viewer's HTTP server: read-only git routes plus a live status stream.
//!
//! Runs on its own port with its own session cookie, entirely separate from the
//! mirror. Two servers sharing a cookie name on one host would let a session
//! for one authenticate against the other.
//!
//! Request handling order is deliberate and load-bearing:
//!
//! 1. **Origin** — a cross-site request is refused before anything else runs.
//! 2. **Auth** — checked *before* the repository is looked up, so an
//!    unauthenticated request cannot probe which ids exist by comparing a 404
//!    against a 401.
//! 3. **Lookup** — an opaque id resolves to a repository, or 404s.
//! 4. **Path validation** — any `path` parameter goes through
//!    [`crate::git::path::resolve_in_workdir`] before touching the filesystem.
//!
//! Each git request opens its own `git2::Repository`. Handler threads are
//! short-lived, so a per-thread cache would be discarded with the thread; the
//! per-repo runtime thread owns the long-lived watching instead.
//!
//! Errors are redacted: git and io messages carry absolute paths, symlink
//! targets, and file sizes, so handlers map them to a fixed public string and
//! log the detail server-side.

use crate::git::diff;
use crate::web::common::auth::{Auth, RateLimiter, SessionStore};
use crate::web::common::conn::{self, ConnectionSlot};
use crate::web::common::http::{self, RequestHead};
use crate::web::common::sse::SseStream;
use crate::web::viewer::catalog::{Catalog, RepoEntry};
use crate::web::viewer::dto::{DiffDto, Envelope, FileDto, LogDto, StatusDto, TreeDto};
use crate::web::viewer::limits;
use anyhow::{Context, Result};
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

/// Distinct from the mirror's cookie: same host, different servers, so a
/// session for one must not authenticate against the other.
pub const VIEWER_SESSION_COOKIE: &str = "nightcrow_viewer_session";

/// How long an idle SSE stream waits before sending a heartbeat. Short enough
/// that a dead socket is noticed promptly, since a write is the only way to
/// find out.
const SSE_HEARTBEAT: Duration = Duration::from_secs(15);

pub struct ViewerState {
    pub catalog: Catalog,
    auth: Auth,
    sessions: SessionStore,
    limiter: RateLimiter,
    connections: Arc<AtomicUsize>,
}

pub struct ViewerServer {
    state: Arc<ViewerState>,
    addr: SocketAddr,
}

impl ViewerServer {
    /// Bind and start accepting. `paths` seeds the catalog; the caller may
    /// replace it later through [`ViewerServer::set_repos`].
    pub fn start(bind: IpAddr, port: u16, auth: Auth, paths: &[String]) -> Result<Self> {
        let listener = TcpListener::bind((bind, port))
            .with_context(|| format!("binding viewer server to {bind}:{port}"))?;
        let addr = listener
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::new(bind, port));

        let state = Arc::new(ViewerState {
            catalog: Catalog::new(),
            auth,
            sessions: SessionStore::new(),
            limiter: RateLimiter::new(),
            connections: Arc::new(AtomicUsize::new(0)),
        });
        state.catalog.set_paths(paths);

        let accept_state = Arc::clone(&state);
        std::thread::Builder::new()
            .name("nightcrow-viewer-accept".into())
            .spawn(move || accept_loop(listener, accept_state))
            .context("spawning viewer accept thread")?;

        Ok(Self { state, addr })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Replace the served repositories. Safe to call from the TUI main loop on
    /// every tab open/close: unchanged paths keep their runtimes and clients.
    pub fn set_repos(&self, paths: &[String]) {
        self.state.catalog.set_paths(paths);
    }

    pub fn shutdown(&self) {
        self.state.catalog.shutdown();
    }
}

impl Drop for ViewerServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(listener: TcpListener, state: Arc<ViewerState>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let Some(slot) =
            ConnectionSlot::acquire(&state.connections, limits::MAX_VIEWER_CONNECTIONS)
        else {
            tracing::debug!("viewer: refusing connection over cap");
            continue;
        };
        let state = Arc::clone(&state);
        let _ = std::thread::Builder::new()
            .name("nightcrow-viewer-conn".into())
            .spawn(move || {
                let _slot = slot;
                handle_connection(stream, state)
            });
    }
}

fn handle_connection(mut stream: TcpStream, state: Arc<ViewerState>) {
    let (head, body) = match conn::read_request(&mut stream) {
        Ok(v) => v,
        Err(err) => {
            tracing::debug!(%err, "viewer: dropping malformed request");
            return;
        }
    };

    // (1) Origin, before anything reads state.
    if !conn::origin_allowed(&head) {
        let _ = stream.write_all(&text_response(
            "403 Forbidden",
            "cross-origin request rejected",
        ));
        return;
    }

    // The login form and its POST are the only routes reachable unauthenticated.
    match (head.method.as_str(), head.path.as_str()) {
        ("POST", "/login") => {
            let _ = stream.write_all(&handle_login(&body, &state));
            return;
        }
        ("GET", "/logout") => {
            let clear =
                format!("{VIEWER_SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0");
            let _ = stream.write_all(&http::redirect("/", &[("Set-Cookie", &clear)]));
            return;
        }
        _ => {}
    }

    // (2) Auth, before any repository is named or looked up.
    if !is_authenticated(&head, &state) {
        let _ = stream.write_all(&match head.path.starts_with("/api/") {
            true => json_error("401 Unauthorized", "authentication required"),
            false => text_response("401 Unauthorized", "authentication required"),
        });
        return;
    }

    // SSE takes over the socket rather than returning a body.
    if head.method == "GET" && head.path == "/api/events" {
        serve_events(stream, &head, &state);
        return;
    }

    let _ = stream.write_all(&route(&head, &state));
}

fn is_authenticated(head: &RequestHead, state: &ViewerState) -> bool {
    head.cookie(VIEWER_SESSION_COOKIE)
        .is_some_and(|token| state.sessions.is_valid(token))
}

fn handle_login(body: &str, state: &ViewerState) -> Vec<u8> {
    if !state.limiter.check_and_record(Instant::now()) {
        return json_error("429 Too Many Requests", "too many attempts");
    }
    let fields = http::parse_form(body);
    let password = http::form_field(&fields, "password").unwrap_or("");
    if !state.auth.verify(password) {
        return json_error("401 Unauthorized", "incorrect password");
    }
    match state.sessions.issue() {
        Ok(token) => {
            let cookie = format!(
                "{VIEWER_SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400"
            );
            json_response("200 OK", "{\"ok\":true}", &[("Set-Cookie", &cookie)])
        }
        Err(err) => {
            tracing::error!(%err, "viewer: could not issue a session token");
            json_error("500 Internal Server Error", "could not start a session")
        }
    }
}

/// Resolve the `repo` parameter to an entry, or produce the 404 response.
fn lookup_repo(head: &RequestHead, state: &ViewerState) -> Result<Arc<RepoEntry>, Vec<u8>> {
    let id = head
        .query_param("repo")
        .ok_or_else(|| json_error("400 Bad Request", "missing repo parameter"))?;
    state
        .catalog
        .get(&id)
        .ok_or_else(|| json_error("404 Not Found", "unknown repository"))
}

/// Map an internal error to a fixed public message, logging the detail.
///
/// git and io errors name absolute paths, symlink targets, and file sizes. The
/// client is told only that the request failed and why in general terms.
fn redact(context: &str, err: &anyhow::Error) -> Vec<u8> {
    tracing::debug!(%err, context, "viewer: request failed");
    json_error("400 Bad Request", "request could not be served")
}

fn route(head: &RequestHead, state: &ViewerState) -> Vec<u8> {
    if head.method != "GET" {
        return json_error("405 Method Not Allowed", "only GET is supported");
    }
    match head.path.as_str() {
        "/api/repos" => {
            let repos = state.catalog.list();
            match serde_json::to_string(&Envelope::new(serde_json::json!({ "repos": repos }))) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode repositories"),
            }
        }
        "/api/status" => with_repo(head, state, |entry| {
            // Served from the runtime's latest snapshot rather than a fresh
            // git call: the runtime is already watching, and this keeps a
            // page refresh from queueing another full status walk.
            match entry.runtime.latest() {
                Some(update) => Ok(json_response("200 OK", &update.json, &[])),
                None => Ok(json_response(
                    "200 OK",
                    &encode(&StatusDto::from_snapshot(&[], None, None, None))?,
                    &[],
                )),
            }
        }),
        "/api/tree" => with_repo(head, state, |entry| {
            let path = head.query_param("path").unwrap_or_default();
            let repo = open_repo(&entry.path)?;
            let workdir = repo
                .workdir()
                .ok_or_else(|| anyhow::anyhow!("bare repository"))?
                .to_path_buf();
            let entries = crate::git::tree::read_children(&repo, &workdir, &path, true)?;
            Ok(json_response(
                "200 OK",
                &encode(&TreeDto::from_entries(&path, &entries))?,
                &[],
            ))
        }),
        "/api/diff" => with_repo(head, state, |entry| {
            let path = required_path(head)?;
            let repo = open_repo(&entry.path)?;
            let hunks = diff::load_file_diff(&repo, &path)?;
            Ok(json_response(
                "200 OK",
                &encode(&DiffDto::from_hunks(&path, &hunks))?,
                &[],
            ))
        }),
        "/api/file" => with_repo(head, state, |entry| {
            let path = required_path(head)?;
            let repo = open_repo(&entry.path)?;
            let content = diff::load_workdir_file(&repo, &path)?;
            Ok(json_response(
                "200 OK",
                &encode(&FileDto::new(&path, &content))?,
                &[],
            ))
        }),
        "/api/log" => with_repo(head, state, |entry| {
            let repo = open_repo(&entry.path)?;
            let commits = diff::load_commit_log(&repo, limits::MAX_LOG_PAGE)?;
            Ok(json_response(
                "200 OK",
                &encode(&LogDto::from_entries(&commits))?,
                &[],
            ))
        }),
        "/api/commit" => with_repo(head, state, |entry| {
            let oid_text = head
                .query_param("oid")
                .ok_or_else(|| anyhow::anyhow!("missing oid parameter"))?;
            let oid = git2::Oid::from_str(&oid_text).context("malformed oid")?;
            let repo = open_repo(&entry.path)?;
            let hunks = diff::load_commit_diff(&repo, oid)?;
            Ok(json_response(
                "200 OK",
                &encode(&DiffDto::from_hunks(&oid_text, &hunks))?,
                &[],
            ))
        }),
        _ => json_error("404 Not Found", "no such route"),
    }
}

/// Look the repository up, validate any `path` parameter, then run `body`.
///
/// Validation happens here rather than in each handler so no route can forget
/// it. Not every downstream touches the filesystem — `load_file_diff` passes
/// the path to git as a pathspec — but a route must not be safe only by
/// accident of which loader it happens to call. A traversal path is refused
/// uniformly, and never echoed back in a response.
fn with_repo(
    head: &RequestHead,
    state: &ViewerState,
    body: impl FnOnce(&RepoEntry) -> Result<Vec<u8>>,
) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    // An absent or empty `path` means "the repository root" for the routes that
    // accept one; anything else has to survive the gate.
    if let Some(path) = head.query_param("path").filter(|p| !p.is_empty())
        && let Err(err) =
            crate::git::path::resolve_in_workdir(std::path::Path::new(&entry.path), &path)
    {
        tracing::debug!(%err, route = %head.path, "viewer: rejected path parameter");
        return json_error("400 Bad Request", "invalid path");
    }
    match body(&entry) {
        Ok(response) => response,
        Err(err) => redact(&head.path, &err),
    }
}

fn required_path(head: &RequestHead) -> Result<String> {
    head.query_param("path")
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing path parameter"))
}

fn open_repo(path: &str) -> Result<git2::Repository> {
    git2::Repository::discover(path).context("failed to open repository")
}

fn encode<T: serde::Serialize>(payload: &T) -> Result<String> {
    serde_json::to_string(&Envelope::new(payload)).context("failed to encode payload")
}

/// Hold the connection open and stream this repository's status.
fn serve_events(mut stream: TcpStream, head: &RequestHead, state: &ViewerState) {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => {
            let _ = stream.write_all(&response);
            return;
        }
    };
    // A stalled reader must not wedge the handler thread forever.
    let _ = stream.set_write_timeout(Some(SSE_HEARTBEAT));

    let subscription = entry.runtime.subscribe();
    let Ok(mut sse) = SseStream::start(stream) else {
        return;
    };
    loop {
        match subscription.next_update(SSE_HEARTBEAT) {
            Some(update) => {
                if sse.send("status", &update.json).is_err() {
                    break;
                }
            }
            // Nothing changed: prove the socket is still alive. This is the
            // only way a closed tab is discovered.
            None => {
                if sse.heartbeat().is_err() {
                    break;
                }
            }
        }
    }
    // `subscription` drops here, unregistering from the fan-out.
}

fn json_response(status: &str, body: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let mut headers = vec![("X-Content-Type-Options", "nosniff")];
    headers.extend_from_slice(extra);
    http::response(
        status,
        "application/json; charset=utf-8",
        &headers,
        body.as_bytes(),
    )
}

fn json_error(status: &str, message: &str) -> Vec<u8> {
    // Message is always a fixed literal from this module, never interpolated
    // from an error, so it needs no escaping and can leak nothing.
    let body = format!("{{\"error\":\"{message}\"}}");
    json_response(status, &body, &[])
}

fn text_response(status: &str, message: &str) -> Vec<u8> {
    http::response(
        status,
        "text/plain; charset=utf-8",
        &[("X-Content-Type-Options", "nosniff")],
        message.as_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{make_repo, run_git};
    use std::io::Read;

    fn server(paths: &[String]) -> ViewerServer {
        ViewerServer::start(
            "127.0.0.1".parse().unwrap(),
            0,
            Auth::from_plaintext("swordfish").unwrap(),
            paths,
        )
        .unwrap()
    }

    /// Send a raw request and return the full response text.
    fn request(addr: SocketAddr, raw: &str) -> String {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut buf = Vec::new();
        let _ = stream.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn get(addr: SocketAddr, path: &str, cookie: Option<&str>) -> String {
        let cookie = match cookie {
            Some(token) => format!("Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n"),
            None => String::new(),
        };
        request(
            addr,
            &format!("GET {path} HTTP/1.1\r\nHost: x\r\n{cookie}Connection: close\r\n\r\n"),
        )
    }

    /// Log in and return the session token.
    fn login(addr: SocketAddr) -> String {
        let body = "password=swordfish";
        let response = request(
            addr,
            &format!(
                "POST /login HTTP/1.1\r\nHost: x\r\n\
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

    fn body_of(response: &str) -> &str {
        response.split("\r\n\r\n").nth(1).unwrap_or("")
    }

    #[test]
    fn api_requires_authentication() {
        let (dir, path) = make_repo();
        let server = server(&[path]);

        let response = get(server.addr(), "/api/repos", None);

        assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn auth_is_checked_before_the_repository_is_looked_up() {
        // An unauthenticated request must not be able to distinguish a real id
        // from a made-up one — that would enumerate the served repositories.
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());
        let real = {
            let listing = get(server.addr(), "/api/repos", Some(&token));
            let value: serde_json::Value = serde_json::from_str(body_of(&listing)).unwrap();
            value["repos"][0]["id"].as_str().unwrap().to_string()
        };

        let known = get(server.addr(), &format!("/api/status?repo={real}"), None);
        let unknown = get(server.addr(), "/api/status?repo=r9999", None);

        assert!(known.starts_with("HTTP/1.1 401"), "got: {known}");
        assert!(unknown.starts_with("HTTP/1.1 401"), "got: {unknown}");
        drop(dir);
    }

    #[test]
    fn a_cross_origin_request_is_refused_before_auth() {
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());

        let response = request(
            server.addr(),
            &format!(
                "GET /api/repos HTTP/1.1\r\nHost: x\r\nOrigin: http://evil.example\r\n\
                 Cookie: {VIEWER_SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
            ),
        );

        assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn the_viewer_cookie_is_distinct_from_the_mirrors() {
        // A mirror session must not authenticate here.
        assert_ne!(
            VIEWER_SESSION_COOKIE,
            crate::web::common::auth::SESSION_COOKIE
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

    #[test]
    fn an_unknown_repo_id_is_a_404_for_an_authenticated_client() {
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());

        let response = get(server.addr(), "/api/status?repo=r9999", Some(&token));

        assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn a_missing_repo_parameter_is_a_400() {
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());

        let response = get(server.addr(), "/api/status", Some(&token));

        assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        drop(dir);
    }

    fn seeded_server() -> (tempfile::TempDir, ViewerServer, String, String) {
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

    #[test]
    fn tree_lists_a_directory_level() {
        let (dir, server, token, id) = seeded_server();

        let response = get(server.addr(), &format!("/api/tree?repo={id}"), Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        let names: Vec<_> = value["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"src"), "got: {names:?}");
        assert!(!names.contains(&".git"), "git metadata must not be listed");
        drop(dir);
    }

    #[test]
    fn a_traversal_path_is_refused_by_every_route_that_takes_one() {
        let (dir, server, token, id) = seeded_server();

        for route in ["tree", "file", "diff"] {
            for attack in ["../../etc/passwd", ".git/config", "src/../.git/config"] {
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

    #[test]
    fn file_returns_worktree_content() {
        let (dir, server, token, id) = seeded_server();

        let response = get(
            server.addr(),
            &format!("/api/file?repo={id}&path=src%2Fmain.rs"),
            Some(&token),
        );
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        assert_eq!(value["content"], "fn main() {}\n");
        assert_eq!(value["truncated"], false);
        drop(dir);
    }

    #[test]
    fn log_returns_commits() {
        let (dir, server, token, id) = seeded_server();

        let response = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        let commits = value["commits"].as_array().unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0]["summary"], "init");
        assert_eq!(
            commits[0]["oid"].as_str().unwrap().len(),
            40,
            "the oid must be hex, not libgit2's own shape"
        );
        drop(dir);
    }

    #[test]
    fn diff_returns_hunks_for_a_changed_file() {
        let (dir, server, token, id) = seeded_server();
        // Mutate the committed file so a worktree diff exists.
        let repo_path = {
            let entry = server.state.catalog.get(&id).unwrap();
            entry.path.clone()
        };
        std::fs::write(
            std::path::Path::new(&repo_path).join("src/main.rs"),
            "fn main() { println!(\"hi\"); }\n",
        )
        .unwrap();

        let response = get(
            server.addr(),
            &format!("/api/diff?repo={id}&path=src%2Fmain.rs"),
            Some(&token),
        );
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        let hunks = value["hunks"].as_array().unwrap();
        assert!(!hunks.is_empty(), "expected a hunk: {value}");
        let kinds: Vec<_> = hunks[0]["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["kind"].as_str().unwrap())
            .collect();
        assert!(
            kinds.contains(&"+") && kinds.contains(&"-"),
            "got: {kinds:?}"
        );
        drop(dir);
    }

    #[test]
    fn a_non_get_method_is_rejected() {
        let (dir, server, token, id) = seeded_server();

        let response = request(
            server.addr(),
            &format!(
                "DELETE /api/status?repo={id} HTTP/1.1\r\nHost: x\r\n\
                 Cookie: {VIEWER_SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
            ),
        );

        assert!(response.starts_with("HTTP/1.1 405"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn the_events_stream_sends_a_status_event() {
        let (dir, server, token, id) = seeded_server();

        let mut stream = TcpStream::connect(server.addr()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .write_all(
                format!(
                    "GET /api/events?repo={id} HTTP/1.1\r\nHost: x\r\n\
                     Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n\r\n"
                )
                .as_bytes(),
            )
            .unwrap();

        // Read until the first dispatched event or the read budget runs out.
        let mut seen = String::new();
        let mut chunk = [0u8; 2048];
        while !seen.contains("event: status") {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => seen.push_str(&String::from_utf8_lossy(&chunk[..n])),
            }
        }

        assert!(
            seen.starts_with("HTTP/1.1 200"),
            "expected a streaming head: {seen}"
        );
        assert!(
            seen.contains("text/event-stream"),
            "expected an SSE content type: {seen}"
        );
        assert!(!seen.contains("Content-Length"), "SSE must not declare one");
        assert!(seen.contains("event: status"), "no status event: {seen}");
        drop(dir);
    }

    #[test]
    fn the_events_stream_requires_auth_and_a_known_repo() {
        let (dir, server, token, _id) = seeded_server();

        let anon = get(server.addr(), "/api/events?repo=r1", None);
        assert!(anon.starts_with("HTTP/1.1 401"), "got: {anon}");

        let unknown = get(server.addr(), "/api/events?repo=r9999", Some(&token));
        assert!(unknown.starts_with("HTTP/1.1 404"), "got: {unknown}");
        drop(dir);
    }
}
