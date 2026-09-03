use super::clone_routes;
use super::http_util::{json_error, json_response, text_response};
use super::mutations::{
    handle_close_repo, handle_mkdir, handle_open_repo, handle_reload_config, handle_reorder_repos,
    handle_set_prefs, handle_write_file,
};
use super::routes::route;
use super::{VIEWER_SESSION_COOKIE, ViewerState};
use crate::web::common::conn::{self, ConnectionSlot};
use crate::web::common::http::{self, RequestHead};
use crate::web::viewer::assets;
use crate::web::viewer::limits;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Instant;

pub(super) fn accept_loop(listener: TcpListener, state: Arc<ViewerState>) {
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
            // A real logout is a top-level navigation (the header link). A
            // framed request here is something embedded — the HTML preview's
            // sandboxed frame navigating itself — trying to end the session
            // out from under the person. Refuse it; absent metadata (an old
            // client) still logs out, which is the safe direction.
            if head.header("sec-fetch-dest") == Some("iframe") {
                let _ = stream.write_all(&text_response("403 Forbidden", "not from this context"));
                return;
            }
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
            &assets::serve(&head.path, head.header("host"))
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
        super::handlers::serve_events(stream, &head, &state);
        return;
    }

    if head.path == "/ws/term" && head.is_websocket_upgrade() {
        super::handlers::serve_terminal(stream, &head, &state);
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

    // Storing a viewer preference. POST for the same reason opening a
    // repository is: a cross-site page cannot trigger it.
    if head.method == "POST" && head.path == "/api/prefs" {
        let _ = stream.write_all(&handle_set_prefs(&body, &state));
        return;
    }

    // Overwriting a working-tree file with edited contents. POST for the same
    // CSRF reasoning, and inside the terminal's trust boundary — see
    // `mutations::file`. The read side of this path is `GET /api/file`.
    if head.method == "POST" && head.path == "/api/file" {
        let _ = stream.write_all(&handle_write_file(&head, &body, &state));
        return;
    }

    // Assembling an editable preview: the editor POSTs the insert list, the
    // server splices it into the file and stashes the result for the frame's
    // GET (below, through `route`) to load once. See `preview::stash_edit`.
    if head.method == "POST" && head.path == "/api/preview/edit" {
        let _ = stream.write_all(&super::preview::stash_edit(&head, &body, &state));
        return;
    }

    // Cloning a remote into the browsed directory. The clone outlives this
    // request, so the response carries a job id the client polls. The URL's
    // scheme is validated before `git` sees it: `ext::` would execute a command.
    if head.method == "POST" && head.path == "/api/clone" {
        let _ = stream.write_all(&clone_routes::handle_clone(&body, &state));
        return;
    }

    if head.method == "GET" && head.path == "/api/clone" {
        let _ = stream.write_all(&clone_routes::handle_clone_status(&head, &state));
        return;
    }

    // Creating a folder inside a browsed directory. POST for the same CSRF
    // reasoning as the others; the new name is validated to a single path
    // segment so it can only land under the directory the picker is showing.
    if head.method == "POST" && head.path == "/api/mkdir" {
        let _ = stream.write_all(&handle_mkdir(&body));
        return;
    }

    // Closing a repository: same trust reasoning as opening. Removes it from
    // the served set and stops its runtime and terminals.
    if head.method == "DELETE" && head.path == "/api/repos" {
        let _ = stream.write_all(&handle_close_repo(&head, &state));
        return;
    }

    if head.method == "POST" && head.path == "/api/repos/order" {
        let _ = stream.write_all(&handle_reorder_repos(&body, &state));
        return;
    }

    // Re-reading config.toml. POST for the same CSRF reasoning as the others,
    // and the body is ignored: what is read is the file on this machine's
    // disk, so this cannot hand the session a configuration of the caller's
    // own making.
    if head.method == "POST" && head.path == "/api/reload" {
        let _ = stream.write_all(&handle_reload_config(&state));
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
                "{VIEWER_SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}",
                state.sessions.cookie_max_age_secs()
            );
            json_response("200 OK", "{\"ok\":true}", &[("Set-Cookie", &cookie)])
        }
        Err(err) => {
            tracing::error!(%err, "viewer: could not issue a session token");
            json_error("500 Internal Server Error", "could not start a session")
        }
    }
}
