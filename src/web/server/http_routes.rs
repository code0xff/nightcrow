use crate::web::common::auth::SESSION_COOKIE;
use crate::web::common::conn;
use crate::web::common::http::{self, RequestHead};
use crate::web::frontend;
use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Instant;

use super::accept::Shared;
use super::ws::serve_websocket;

pub(super) fn handle_connection(mut stream: TcpStream, shared: Arc<Shared>) {
    let (head, body) = match conn::read_request(&mut stream) {
        Ok(v) => v,
        Err(err) => {
            tracing::debug!(%err, "web: dropping malformed request");
            return;
        }
    };

    let authed = is_authenticated(&head, &shared);

    if head.path == "/ws" && head.is_websocket_upgrade() {
        if !authed {
            let _ = stream.write_all(&http::response(
                "401 Unauthorized",
                "text/plain; charset=utf-8",
                &[],
                b"authentication required",
            ));
            return;
        }
        // Defense-in-depth against cross-site WebSocket hijacking: reject a
        // browser upgrade whose Origin is not this server. SameSite=Strict
        // already keeps the session cookie off cross-site requests, so a
        // hijack fails auth anyway; this refuses it outright. A missing Origin
        // (native, non-browser clients) is allowed — such a client cannot
        // carry a victim's cookie.
        if !conn::origin_allowed(&head) {
            let _ = stream.write_all(&http::response(
                "403 Forbidden",
                "text/plain; charset=utf-8",
                &[],
                b"cross-origin websocket rejected",
            ));
            return;
        }
        serve_websocket(stream, &head, shared);
        return;
    }

    let response = route_http(&head, &body, &shared);
    let _ = stream.write_all(&response);
}

fn is_authenticated(head: &RequestHead, shared: &Shared) -> bool {
    head.cookie(SESSION_COOKIE)
        .is_some_and(|token| shared.sessions.is_valid(token))
}

fn route_http(head: &RequestHead, body: &str, shared: &Shared) -> Vec<u8> {
    match (head.method.as_str(), head.path.as_str()) {
        ("GET", "/") => {
            if is_authenticated(head, shared) {
                http::html("200 OK", frontend::APP_HTML)
            } else {
                http::html("200 OK", &frontend::login_page(None))
            }
        }
        ("POST", "/login") => handle_login(body, shared),
        ("GET", "/logout") => {
            // Revoke server-side, not just in the browser: cookies are not
            // port-isolated, so any other loopback service is same-site here
            // and could have read the token before it was cleared.
            if let Some(token) = head.cookie(SESSION_COOKIE) {
                shared.sessions.revoke(token);
            }
            let clear = format!("{SESSION_COOKIE}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0");
            http::redirect("/", &[("Set-Cookie", &clear)])
        }
        // Public vendored renderer assets (MIT xterm.js); no secrets.
        ("GET", "/vendor/xterm.js") => http::response(
            "200 OK",
            "application/javascript; charset=utf-8",
            &[],
            frontend::XTERM_JS.as_bytes(),
        ),
        ("GET", "/vendor/xterm.css") => http::response(
            "200 OK",
            "text/css; charset=utf-8",
            &[],
            frontend::XTERM_CSS.as_bytes(),
        ),
        // The favicon; public, no secrets.
        ("GET", "/crow.svg") => http::response(
            "200 OK",
            "image/svg+xml; charset=utf-8",
            &[],
            frontend::CROW_SVG.as_bytes(),
        ),
        // The header/login mark; public, referenced by the login page pre-auth.
        ("GET", "/crow-mono.svg") => http::response(
            "200 OK",
            "image/svg+xml; charset=utf-8",
            &[],
            frontend::CROW_MONO_SVG.as_bytes(),
        ),
        _ => http::html("404 Not Found", "<h1>404 Not Found</h1>"),
    }
}

fn handle_login(body: &str, shared: &Shared) -> Vec<u8> {
    if !shared.limiter.check_and_record(Instant::now()) {
        return http::response(
            "429 Too Many Requests",
            "text/html; charset=utf-8",
            &[],
            frontend::login_page(Some("Too many attempts — wait a minute and try again."))
                .as_bytes(),
        );
    }

    let fields = http::parse_form(body);
    let password = http::form_field(&fields, "password").unwrap_or("");
    if !shared.auth.verify(password) {
        return http::response(
            "401 Unauthorized",
            "text/html; charset=utf-8",
            &[],
            frontend::login_page(Some("Incorrect password.")).as_bytes(),
        );
    }

    match shared.sessions.issue() {
        Ok(token) => {
            let cookie = format!("{SESSION_COOKIE}={token}; HttpOnly; SameSite=Strict; Path=/");
            http::redirect("/", &[("Set-Cookie", &cookie)])
        }
        Err(err) => {
            tracing::error!(%err, "web: failed to mint session token");
            http::response(
                "500 Internal Server Error",
                "text/html; charset=utf-8",
                &[],
                b"<h1>internal error</h1>",
            )
        }
    }
}