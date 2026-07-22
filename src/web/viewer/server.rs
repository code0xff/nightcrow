//! The viewer's HTTP server: read-only git routes plus a live status stream.
//!
//! Runs on its own port with its own session cookie, entirely separate from the
//! mirror. Two servers sharing a cookie name on one host would let a session
//! for one authenticate against the other.
//!
//! Request handling order is deliberate and load-bearing:
//!
//! 1. **Host, then Origin** — a rebound name or a cross-site request is
//!    refused before anything else runs. Origin alone only proves Origin and
//!    Host agree, which a DNS-rebinding attacker satisfies trivially.
//! 2. **Static assets** — the built bundle is public. It holds no repository
//!    data and renders the login form, so gating it would leave no way in.
//! 3. **Auth** — checked *before* the repository is looked up, so an
//!    unauthenticated request cannot probe which ids exist by comparing a 404
//!    against a 401.
//! 4. **Lookup** — an opaque id resolves to a repository, or 404s.
//! 5. **Path validation** — any `path` parameter goes through
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
use crate::web::viewer::assets;
use crate::web::viewer::catalog::{AddOutcome, Catalog, RepoEntry};
use crate::web::viewer::dto::{
    BrowseDto, BrowseEntryDto, CommitFilesDto, DiffDto, Envelope, FileDto, LogDto, StatusDto,
    TreeDto, TreeSearchDto,
};
use crate::web::viewer::limits;
use crate::web::viewer::terminal::{self, ClientMessage, TerminalFrame};
use anyhow::{Context, Result};
use std::io::Write;
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};
use tungstenite::Message;

/// Distinct from the mirror's cookie: same host, different servers, so a
/// session for one must not authenticate against the other.
pub const VIEWER_SESSION_COOKIE: &str = "nightcrow_viewer_session";

/// How long an idle SSE stream waits before sending a heartbeat. Short enough
/// that a dead socket is noticed promptly, since a write is the only way to
/// find out.
const SSE_HEARTBEAT: Duration = Duration::from_secs(15);

/// Read timeout on a terminal socket. Bounds how long queued output waits
/// behind a blocked read; terminal latency is felt directly.
const TERM_POLL_TIMEOUT: Duration = Duration::from_millis(10);

pub struct ViewerState {
    pub catalog: Catalog,
    /// Whether the listener is on a loopback address. Gates the Host check:
    /// off-loopback, the operator owns the network path and may front this
    /// with a proxy under any name.
    bound_loopback: bool,
    auth: Auth,
    sessions: SessionStore,
    limiter: RateLimiter,
    connections: Arc<AtomicUsize>,
    /// Whether catalog changes are mirrored to the shared workspace file. On in
    /// headless `serve` (so opens/closes are remembered), off alongside the TUI
    /// (which owns that file).
    persist: bool,
    /// The TUI's recently-touched settings, served to the client so the file
    /// list fades on the same window the TUI does instead of a second, silently
    /// diverging default. `auto_follow` is not sent: it moves the TUI's
    /// selection, which is a keyboard-driven notion the viewer has no analogue
    /// for.
    hot: crate::config::AgentIndicatorConfig,
}

pub struct ViewerServer {
    state: Arc<ViewerState>,
    addr: SocketAddr,
}

impl ViewerServer {
    /// Bind and start from `[web_viewer]`, building the password verifier from
    /// either `hashed_password` or `password`.
    pub fn start_from_config(
        viewer: &crate::config::WebViewerConfig,
        agent_indicator: &crate::config::AgentIndicatorConfig,
        paths: &[String],
        persist: bool,
        startup_commands: Vec<String>,
    ) -> Result<Self> {
        let auth = if let Some(hash) = viewer.hashed_password.as_deref() {
            Auth::from_hashed(hash)?
        } else if let Some(password) = viewer.password.as_deref().filter(|p| !p.is_empty()) {
            Auth::from_plaintext(password)?
        } else {
            anyhow::bail!("web viewer is enabled but no password or hashed_password is configured");
        };
        let bind: IpAddr = viewer.bind.parse().with_context(|| {
            format!(
                "web_viewer.bind {:?} is not a valid IP address",
                viewer.bind
            )
        })?;
        Self::start(
            bind,
            viewer.port,
            auth,
            paths,
            persist,
            startup_commands,
            agent_indicator.clone(),
        )
    }

    /// Bind and start accepting. `paths` seeds the catalog; the caller may
    /// replace it later through [`ViewerServer::set_repos`]. `persist` mirrors
    /// catalog changes into the shared workspace file (headless `serve` only).
    pub fn start(
        bind: IpAddr,
        port: u16,
        auth: Auth,
        paths: &[String],
        persist: bool,
        startup_commands: Vec<String>,
        hot: crate::config::AgentIndicatorConfig,
    ) -> Result<Self> {
        let listener = TcpListener::bind((bind, port))
            .with_context(|| format!("binding viewer server to {bind}:{port}"))?;
        let addr = listener
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::new(bind, port));

        let state = Arc::new(ViewerState {
            catalog: Catalog::with_startup(startup_commands),
            bound_loopback: bind.is_loopback(),
            auth,
            sessions: SessionStore::new(),
            limiter: RateLimiter::new(),
            connections: Arc::new(AtomicUsize::new(0)),
            persist,
            hot,
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

    // (1) Host, then Origin — both before anything reads state. Origin only
    // proves the two agree, which a DNS-rebound attacker controls outright.
    if !conn::host_allowed(&head, state.bound_loopback) {
        let _ = stream.write_all(&text_response("403 Forbidden", "unexpected host"));
        return;
    }
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
            // Revoke server-side, not just in the browser: cookies are not
            // port-isolated, so any other loopback service is same-site here
            // and could have read the token before it was cleared.
            if let Some(token) = head.cookie(VIEWER_SESSION_COOKIE) {
                state.sessions.revoke(token);
            }
            let clear =
                format!("{VIEWER_SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0");
            let _ = stream.write_all(&http::redirect("/", &[("Set-Cookie", &clear)]));
            return;
        }
        _ => {}
    }

    // (2) The static bundle is served unauthenticated. It carries no
    // repository data — it is the shell that renders the login form and then
    // calls the API, which *is* gated. Requiring a session to fetch it would
    // mean the user could never reach a login screen at all.
    if head.method == "GET" && !head.path.starts_with("/api/") && head.path != "/ws/term" {
        let _ = stream.write_all(
            &assets::serve(&head.path)
                .unwrap_or_else(|| text_response("404 Not Found", "frontend not built")),
        );
        return;
    }

    // (3) Auth, before any repository is named or looked up.
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

    if head.path == "/ws/term" && head.is_websocket_upgrade() {
        serve_terminal(stream, &head, &state);
        return;
    }

    // Opening a repository is the one state-changing route. It is a POST, so a
    // cross-site page cannot trigger it (Origin was already checked, and the
    // session cookie is SameSite=Strict). An authenticated user can already
    // open a shell here, so pointing the viewer at another local directory
    // stays within the same trust boundary.
    if head.method == "POST" && head.path == "/api/repos" {
        let _ = stream.write_all(&handle_open_repo(&body, &state));
        return;
    }

    // Closing a repository: same trust reasoning as opening. Removes it from
    // the served set and stops its runtime and terminals.
    if head.method == "DELETE" && head.path == "/api/repos" {
        let _ = stream.write_all(&handle_close_repo(&head, &state));
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

#[derive(serde::Deserialize)]
struct OpenRequest {
    path: String,
}

/// Open a repository from the browser and add it to the served catalog.
///
/// The path is user-supplied but the response is public, so a bad path yields a
/// generic message rather than echoing what was tried.
fn handle_open_repo(body: &str, state: &ViewerState) -> Vec<u8> {
    let request: OpenRequest = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => return json_error("400 Bad Request", "expected a JSON body with a path"),
    };
    let raw = request.path.trim();
    if raw.is_empty() {
        return json_error("400 Bad Request", "a path is required");
    }
    let expanded = crate::util::expand_tilde(raw);
    // is_dir() follows symlinks and is false for a missing path — either way it
    // cannot be served.
    if !expanded.is_dir() {
        return json_error("400 Bad Request", "no such directory");
    }
    let resolved = crate::git::resolve_repo_path(&expanded)
        .to_string_lossy()
        .into_owned();

    match state.catalog.add_path(resolved, crate::workspace::MAX_PROJECTS) {
        AddOutcome::Added(repo) => {
            persist_workspace(state);
            match serde_json::to_string(&Envelope::new(serde_json::json!({ "repo": repo }))) {
                Ok(json) => json_response("200 OK", &json, &[]),
                Err(_) => json_error("500 Internal Server Error", "could not encode repository"),
            }
        }
        AddOutcome::TooMany => json_error(
            "409 Conflict",
            "the maximum number of repositories is already open",
        ),
    }
}

/// Close a repository named by the `repo` id and return the updated set.
///
/// Idempotent from the client's view: an unknown id is a 404, a known one is
/// removed and its runtime/terminals stopped by the catalog rebuild.
fn handle_close_repo(head: &RequestHead, state: &ViewerState) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    state.catalog.remove_path(&entry.path);
    persist_workspace(state);
    let repos = state.catalog.list();
    match serde_json::to_string(&Envelope::new(serde_json::json!({ "repos": repos }))) {
        Ok(json) => json_response("200 OK", &json, &[]),
        Err(_) => json_error("500 Internal Server Error", "could not encode repositories"),
    }
}

/// Mirror the served set into the shared workspace file so the next launch —
/// TUI, mirror, or viewer — starts with the same projects. No-op unless the
/// server was started with `persist` (headless `serve`); alongside the TUI,
/// the TUI owns that file. The existing per-repo view state and active tab are
/// preserved; only the open-repo list is rewritten.
fn persist_workspace(state: &ViewerState) {
    if !state.persist {
        return;
    }
    let mut ws = crate::session::load_workspace().unwrap_or_default();
    ws.repos = state.catalog.paths();
    if ws.active >= ws.repos.len() {
        ws.active = 0;
    }
    crate::session::save_workspace(&ws);
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
            // The recently-touched settings ride along with the repository list
            // rather than on `/api/status`: they are per-server and unchanging,
            // and the status payload is a hot, deduplicated stream that should
            // not carry configuration.
            match serde_json::to_string(&Envelope::new(serde_json::json!({
                "repos": repos,
                "hot": {
                    "enabled": state.hot.enabled,
                    "window_secs": state.hot.hot_window_secs,
                },
            }))) {
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
                    &encode(&StatusDto::from_snapshot(
                        &[],
                        None,
                        None,
                        None,
                        &std::collections::HashMap::new(),
                    ))?,
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
        "/api/tree/search" => with_repo(head, state, |entry| {
            let query = head.query_param("q").unwrap_or_default();
            // An empty query would match every entry, and an over-long one is not
            // a real filename search; both short-circuit to an empty result so the
            // walk never runs on degenerate input.
            let (matches, truncated) =
                if query.is_empty() || query.len() > limits::MAX_TREE_SEARCH_QUERY_BYTES {
                    (Vec::new(), false)
                } else {
                    let repo = open_repo(&entry.path)?;
                    let workdir = repo
                        .workdir()
                        .ok_or_else(|| anyhow::anyhow!("bare repository"))?
                        .to_path_buf();
                    crate::git::tree::search_tree(
                        &repo,
                        &workdir,
                        &query,
                        limits::MAX_TREE_SEARCH_DEPTH,
                        limits::MAX_TREE_SEARCH_VISITS,
                        limits::MAX_TREE_SEARCH_RESULTS,
                    )?
                };
            Ok(json_response(
                "200 OK",
                &encode(&TreeSearchDto::new(&query, &matches, truncated))?,
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
            let oid = required_oid(head)?;
            let oid_text = oid.to_string();
            let repo = open_repo(&entry.path)?;
            let hunks = diff::load_commit_diff(&repo, oid)?;
            Ok(json_response(
                "200 OK",
                &encode(&DiffDto::from_hunks(&oid_text, &hunks))?,
                &[],
            ))
        }),
        "/api/commit/files" => with_repo(head, state, |entry| {
            let oid = required_oid(head)?;
            let repo = open_repo(&entry.path)?;
            let files = diff::load_commit_files(&repo, oid)?;
            Ok(json_response(
                "200 OK",
                &encode(&CommitFilesDto::from_entries(&files))?,
                &[],
            ))
        }),
        "/api/commit/file-diff" => with_repo_commit_path(head, state, |entry, path| {
            let oid = required_oid(head)?;
            let repo = open_repo(&entry.path)?;
            let hunks = diff::load_commit_file_diff(&repo, oid, path)?;
            Ok(json_response(
                "200 OK",
                &encode(&DiffDto::from_hunks(path, &hunks))?,
                &[],
            ))
        }),
        "/api/browse" => browse(head),
        _ => json_error("404 Not Found", "no such route"),
    }
}

/// List the server sub-directories under `path` (home when absent) for the
/// folder picker. Directories only, hidden ones skipped; each is flagged when
/// it looks like a git worktree. Deliberately unconfined — the picker browses
/// the server to find a repo to open — but reachable only authenticated and at
/// the same trust as the terminal.
fn browse(head: &RequestHead) -> Vec<u8> {
    let start = match head.query_param("path").filter(|p| !p.is_empty()) {
        Some(path) => std::path::PathBuf::from(path),
        None => dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/")),
    };
    match list_directories(&start) {
        Ok(dto) => match serde_json::to_string(&Envelope::new(dto)) {
            Ok(json) => json_response("200 OK", &json, &[]),
            Err(_) => json_error("500 Internal Server Error", "could not encode listing"),
        },
        Err(err) => redact(&head.path, &err),
    }
}

fn list_directories(path: &std::path::Path) -> anyhow::Result<BrowseDto> {
    use anyhow::Context;
    let canonical = path
        .canonicalize()
        .with_context(|| "path could not be resolved")?;
    if !canonical.is_dir() {
        anyhow::bail!("not a directory");
    }
    let mut entries: Vec<BrowseEntryDto> = Vec::new();
    let mut truncated = false;
    for entry in std::fs::read_dir(&canonical).with_context(|| "directory is not readable")? {
        let Ok(entry) = entry else { continue };
        // `file_type` does not follow symlinks, so a symlinked directory is
        // skipped rather than risking a browse loop.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if entries.len() >= limits::MAX_TREE_ENTRIES {
            truncated = true;
            break;
        }
        let is_repo = entry.path().join(".git").exists();
        entries.push(BrowseEntryDto { name, is_repo });
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(BrowseDto {
        path: canonical.to_string_lossy().into_owned(),
        parent: canonical.parent().map(|p| p.to_string_lossy().into_owned()),
        entries,
        truncated,
    })
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

/// Variant of [`with_repo`] for a path inside a historical git object.
///
/// A deleted commit path cannot be resolved in the current worktree, so this
/// validates its syntax without statting it. The route passes it only to an
/// exact git pathspec; it never opens a filesystem path.
fn with_repo_commit_path(
    head: &RequestHead,
    state: &ViewerState,
    body: impl FnOnce(&RepoEntry, &str) -> Result<Vec<u8>>,
) -> Vec<u8> {
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => return response,
    };
    let path = match required_path(head) {
        Ok(path) => path,
        Err(err) => return redact(&head.path, &err),
    };
    if let Err(err) = crate::git::path::validate_commit_path(&path) {
        tracing::debug!(%err, route = %head.path, "viewer: rejected historical path parameter");
        return json_error("400 Bad Request", "invalid path");
    }
    match body(&entry, &path) {
        Ok(response) => response,
        Err(err) => redact(&head.path, &err),
    }
}

fn required_path(head: &RequestHead) -> Result<String> {
    head.query_param("path")
        .filter(|p| !p.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing path parameter"))
}

fn required_oid(head: &RequestHead) -> Result<git2::Oid> {
    let oid_text = head
        .query_param("oid")
        .ok_or_else(|| anyhow::anyhow!("missing oid parameter"))?;
    git2::Oid::from_str(&oid_text).context("malformed oid")
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

/// Hand this connection to the repository's terminal hub.
///
/// Auth and Origin were already enforced by `handle_connection`, before the
/// repository was named — a terminal is effectively a shell, so the upgrade
/// must never be reachable ahead of those checks.
fn serve_terminal(stream: TcpStream, head: &RequestHead, state: &ViewerState) {
    let mut stream = stream;
    let entry = match lookup_repo(head, state) {
        Ok(entry) => entry,
        Err(response) => {
            let _ = stream.write_all(&response);
            return;
        }
    };
    // Without a read timeout, `ws.read()` blocks and terminal output would
    // only flush when the user happened to type. The timeout turns the loop
    // into a poll that services both directions.
    let _ = stream.set_read_timeout(Some(TERM_POLL_TIMEOUT));
    let _ = stream.set_write_timeout(Some(SSE_HEARTBEAT));
    let Some(mut ws) = conn::websocket_handshake(stream, head) else {
        return;
    };
    let session = entry.terminals.connect();

    loop {
        // Drain everything queued for us before blocking on the socket, so
        // output is not held back waiting for the client to say something.
        let mut wrote = false;
        while let Some(frame) = session.next_frame(Duration::from_millis(1)) {
            let message = match frame {
                TerminalFrame::Output { pane, data } => {
                    Message::Binary(terminal::encode_output(pane, &data).into())
                }
                TerminalFrame::Control(json) => Message::Text(json.into()),
            };
            if ws.send(message).is_err() {
                return;
            }
            wrote = true;
        }
        if wrote && ws.flush().is_err() {
            return;
        }

        match ws.read() {
            Ok(Message::Text(text)) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(message) => session.dispatch(message),
                // A malformed frame is dropped, not fatal: a client bug should
                // not take the terminal down with it.
                Err(err) => tracing::debug!(%err, "viewer: bad terminal message"),
            },
            Ok(Message::Close(_)) => return,
            Ok(_) => {}
            // A poll timeout surfaces as WouldBlock on macOS and TimedOut on
            // Linux; neither means the client is gone.
            Err(tungstenite::Error::Io(err))
                if matches!(
                    err.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
    }
    // `session` drops here, unregistering from the hub.
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
        server_with_hot(paths, crate::config::AgentIndicatorConfig::default())
    }

    fn server_with_hot(
        paths: &[String],
        hot: crate::config::AgentIndicatorConfig,
    ) -> ViewerServer {
        ViewerServer::start(
            "127.0.0.1".parse().unwrap(),
            0,
            Auth::from_plaintext("swordfish").unwrap(),
            paths,
            // Never persist from tests — they must not touch the real
            // ~/.nightcrow/workspace.json.
            false,
            Vec::new(),
            hot,
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
            &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{cookie}Connection: close\r\n\r\n"),
        )
    }

    fn post(addr: SocketAddr, path: &str, body: &str, cookie: Option<&str>) -> String {
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

    fn delete(addr: SocketAddr, path: &str, cookie: Option<&str>) -> String {
        let cookie = match cookie {
            Some(token) => format!("Cookie: {VIEWER_SESSION_COOKIE}={token}\r\n"),
            None => String::new(),
        };
        request(
            addr,
            &format!(
                "DELETE {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{cookie}Connection: close\r\n\r\n"
            ),
        )
    }

    /// Log in and return the session token.
    fn login(addr: SocketAddr) -> String {
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

    fn body_of(response: &str) -> &str {
        response.split("\r\n\r\n").nth(1).unwrap_or("")
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
        let server = server_with_hot(
            &[],
            crate::config::AgentIndicatorConfig {
                enabled: false,
                hot_window_secs: 42,
                auto_follow: true,
            },
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
    fn api_requires_authentication() {
        let (dir, path) = make_repo();
        let server = server(&[path]);

        let response = get(server.addr(), "/api/repos", None);

        assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn opening_a_repository_adds_it_to_the_served_set() {
        // Start empty, the way `serve` with no --repo now does, then open a
        // repository from the browser.
        let server = server(&[]);
        let token = login(server.addr());
        let (dir, path) = make_repo();
        let body = format!("{{\"path\":{}}}", serde_json::to_string(&path).unwrap());

        let opened = post(server.addr(), "/api/repos", &body, Some(&token));
        assert!(opened.starts_with("HTTP/1.1 200"), "got: {opened}");

        let list = get(server.addr(), "/api/repos", Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
        assert_eq!(
            value["repos"].as_array().unwrap().len(),
            1,
            "the opened repository must appear in the served set"
        );
        drop(dir);
    }

    #[test]
    fn opening_a_repository_requires_authentication() {
        let server = server(&[]);
        let (dir, path) = make_repo();
        let body = format!("{{\"path\":{}}}", serde_json::to_string(&path).unwrap());

        let response = post(server.addr(), "/api/repos", &body, None);

        assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn browse_lists_subdirectories_and_flags_repos() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("alpha")).unwrap();
        std::fs::create_dir_all(root.path().join("beta").join(".git")).unwrap();
        std::fs::write(root.path().join("afile.txt"), b"x").unwrap();
        let server = server(&[]);
        let token = login(server.addr());

        let path = root.path().to_string_lossy();
        let response = get(
            server.addr(),
            &format!("/api/browse?path={path}"),
            Some(&token),
        );
        assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");

        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
        let list = value["entries"].as_array().unwrap();
        let names: Vec<&str> = list.iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert!(
            names.contains(&"alpha") && names.contains(&"beta"),
            "expected sub-directories, got: {names:?}"
        );
        assert!(!names.contains(&"afile.txt"), "files must be excluded");
        let beta = list.iter().find(|e| e["name"] == "beta").unwrap();
        assert_eq!(beta["is_repo"], true, "a .git folder marks a repo");
    }

    #[test]
    fn closing_a_repository_removes_it_from_the_served_set() {
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());

        let list = get(server.addr(), "/api/repos", Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&list)).unwrap();
        let id = value["repos"][0]["id"].as_str().unwrap().to_string();

        let closed = delete(server.addr(), &format!("/api/repos?repo={id}"), Some(&token));
        assert!(closed.starts_with("HTTP/1.1 200"), "got: {closed}");

        let after = get(server.addr(), "/api/repos", Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&after)).unwrap();
        assert_eq!(
            value["repos"].as_array().unwrap().len(),
            0,
            "the closed repository must be gone from the served set"
        );
        drop(dir);
    }

    #[test]
    fn closing_an_unknown_repository_is_a_404() {
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());

        let response = delete(server.addr(), "/api/repos?repo=nope", Some(&token));

        assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn opening_a_nonexistent_path_is_rejected() {
        let server = server(&[]);
        let token = login(server.addr());

        let response = post(
            server.addr(),
            "/api/repos",
            "{\"path\":\"/definitely/not/a/real/directory\"}",
            Some(&token),
        );

        assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
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
    fn a_rebound_host_is_refused_on_a_loopback_bind() {
        // DNS rebinding: the attacker controls Origin *and* Host, so they
        // agree and the origin check alone would pass. Only the Host check
        // denies the same-origin foothold.
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());

        let response = request(
            server.addr(),
            &format!(
                "GET /api/repos HTTP/1.1\r\nHost: evil.example\r\n\
                 Origin: http://evil.example\r\n\
                 Cookie: {VIEWER_SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
            ),
        );

        assert!(response.starts_with("HTTP/1.1 403"), "got: {response}");
        drop(dir);
    }

    #[test]
    fn logout_revokes_the_session_server_side() {
        // Clearing the cookie is not enough: cookies are not port-isolated, so
        // another loopback service is same-site and may already hold the token.
        let (dir, path) = make_repo();
        let server = server(&[path]);
        let token = login(server.addr());
        assert!(get(server.addr(), "/api/repos", Some(&token)).starts_with("HTTP/1.1 200"));

        get(server.addr(), "/logout", Some(&token));

        let after = get(server.addr(), "/api/repos", Some(&token));
        assert!(
            after.starts_with("HTTP/1.1 401"),
            "the token must stop working immediately: {after}"
        );
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
                "GET /api/repos HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://evil.example\r\n\
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
    fn tree_search_finds_a_nested_file_by_name() {
        let (dir, server, token, id) = seeded_server();

        let response = get(
            server.addr(),
            &format!("/api/tree/search?repo={id}&q=main"),
            Some(&token),
        );
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        let paths: Vec<_> = value["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["path"].as_str().unwrap())
            .collect();
        // The match lives one level down, which the single-level /api/tree could
        // not surface.
        assert_eq!(paths, vec!["src/main.rs"]);
        assert_eq!(value["truncated"], false);
        drop(dir);
    }

    #[test]
    fn tree_search_with_an_empty_query_returns_no_matches() {
        let (dir, server, token, id) = seeded_server();

        let response = get(
            server.addr(),
            &format!("/api/tree/search?repo={id}&q="),
            Some(&token),
        );
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        assert!(value["matches"].as_array().unwrap().is_empty());
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

        // Content is returned as per-line, syntax-highlighted spans. Rebuild the
        // text from them and confirm it round-trips.
        let text: String = value["lines"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|line| line.as_array().unwrap())
            .map(|span| span["t"].as_str().unwrap())
            .collect::<Vec<_>>()
            .join("");
        assert_eq!(text, "fn main() {}");
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
    fn commit_files_returns_the_selected_commits_changed_paths() {
        let (dir, server, token, id) = seeded_server();
        let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
        let oid = value["commits"][0]["oid"].as_str().unwrap();

        let response = get(
            server.addr(),
            &format!("/api/commit/files?repo={id}&oid={oid}"),
            Some(&token),
        );
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
        assert_eq!(value["files"][0]["path"], "src/main.rs");
        assert_eq!(value["files"][0]["index"], "A");
        assert_eq!(value["files"][0]["worktree"], " ");
        assert_eq!(value["truncated"], false);
        drop(dir);
    }

    #[test]
    fn commit_file_diff_returns_only_the_selected_path() {
        let (dir, server, token, id) = seeded_server();
        let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
        let oid = value["commits"][0]["oid"].as_str().unwrap();

        let response = get(
            server.addr(),
            &format!("/api/commit/file-diff?repo={id}&oid={oid}&path=src%2Fmain.rs"),
            Some(&token),
        );
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
        assert_eq!(value["path"], "src/main.rs");
        assert!(
            value["hunks"].as_array().unwrap().iter().any(|hunk| {
                hunk["file_path"] == "src/main.rs" && !hunk["lines"].as_array().unwrap().is_empty()
            }),
            "expected a diff for just src/main.rs: {value}"
        );
        drop(dir);
    }

    #[test]
    fn commit_file_diff_allows_a_deleted_path_without_worktree_lookup() {
        let (dir, server, token, id) = seeded_server();
        let repo_path = {
            let entry = server.state.catalog.get(&id).unwrap();
            entry.path.clone()
        };
        let gone = std::path::Path::new(&repo_path).join("gone.txt");
        std::fs::write(&gone, "before delete\n").unwrap();
        run_git(&repo_path, &["add", "gone.txt"]);
        run_git(&repo_path, &["commit", "-m", "add gone"]);
        run_git(&repo_path, &["rm", "gone.txt"]);
        run_git(&repo_path, &["commit", "-m", "delete gone"]);
        assert!(!gone.exists(), "test precondition: file must be deleted");

        let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
        let oid = value["commits"][0]["oid"].as_str().unwrap();
        let response = get(
            server.addr(),
            &format!("/api/commit/file-diff?repo={id}&oid={oid}&path=gone.txt"),
            Some(&token),
        );
        let value: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();

        assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
        assert!(
            value["hunks"]
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|h| h["lines"].as_array().unwrap())
                .any(|line| line["kind"] == "-"),
            "expected a removal line: {value}"
        );
        drop(dir);
    }

    #[test]
    fn commit_file_diff_rejects_traversal_without_requiring_a_worktree_file() {
        let (dir, server, token, id) = seeded_server();
        let log = get(server.addr(), &format!("/api/log?repo={id}"), Some(&token));
        let value: serde_json::Value = serde_json::from_str(body_of(&log)).unwrap();
        let oid = value["commits"][0]["oid"].as_str().unwrap();

        for attack in ["..%2Fsecret", ".git%2Fconfig", "src%2F..%2Fx"] {
            let response = get(
                server.addr(),
                &format!("/api/commit/file-diff?repo={id}&oid={oid}&path={attack}"),
                Some(&token),
            );
            assert!(
                response.starts_with("HTTP/1.1 400"),
                "historical route accepted {attack:?}: {response}"
            );
        }
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
                "DELETE /api/status?repo={id} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
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
                    "GET /api/events?repo={id} HTTP/1.1\r\nHost: 127.0.0.1\r\n\
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
    fn the_terminal_socket_creates_a_pane_and_streams_its_output() {
        use tungstenite::client::IntoClientRequest;

        let (dir, server, token, id) = seeded_server();
        let mut request = format!("ws://{}/ws/term?repo={id}", server.addr())
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Cookie",
            format!("{VIEWER_SESSION_COOKIE}={token}").parse().unwrap(),
        );
        let (mut ws, _) = tungstenite::connect(request).expect("terminal upgrade");

        ws.send(tungstenite::Message::Text(
            r#"{"type":"create","rows":24,"cols":80}"#.into(),
        ))
        .unwrap();

        // Expect created control frames, then real PTY bytes tagged with a pane
        // id — proving the multiplexing round-trips end to end. More than one
        // pane can appear: the first connect also spawns the default startup
        // terminal, so track every announced pane and require output for one.
        let mut created = std::collections::HashSet::new();
        let mut saw_output = false;
        for _ in 0..40 {
            match ws.read() {
                Ok(tungstenite::Message::Text(text)) if text.contains("created") => {
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if let Some(pane) = value["pane"].as_u64() {
                        created.insert(pane as u32);
                    }
                }
                Ok(tungstenite::Message::Binary(bytes)) => {
                    let (pane, data) = terminal::decode_output(&bytes).expect("a tagged frame");
                    assert!(created.contains(&pane), "output for an unannounced pane");
                    if !data.is_empty() {
                        saw_output = true;
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }

        assert!(!created.is_empty(), "no created frame");
        assert!(saw_output, "no PTY output reached the socket");
        drop(dir);
    }

    #[test]
    fn the_terminal_socket_requires_auth_and_a_known_repo() {
        let (dir, server, token, _id) = seeded_server();

        let anon = get(server.addr(), "/ws/term?repo=r1", None);
        assert!(anon.starts_with("HTTP/1.1 401"), "got: {anon}");

        // Authenticated but unknown: refused before any upgrade happens.
        let unknown = request(
            server.addr(),
            &format!(
                "GET /ws/term?repo=r9999 HTTP/1.1\r\nHost: 127.0.0.1\r\nUpgrade: websocket\r\n\
                 Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                 Sec-WebSocket-Version: 13\r\nCookie: {VIEWER_SESSION_COOKIE}={token}\r\n\r\n"
            ),
        );
        assert!(unknown.starts_with("HTTP/1.1 404"), "got: {unknown}");
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
