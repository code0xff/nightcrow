//! Synchronous WebSocket/HTTP server for the web mirror.
//!
//! No async runtime: an accept thread spawns one handler thread per connection.
//! Handler threads exchange work with the single-threaded main loop over
//! channels — browser input flows in over an `mpsc` the main loop drains, and
//! encoded screen frames flow out over a per-client channel. The `App` is never
//! shared across threads; only bytes and decoded input events cross the boundary.

use crate::web::auth::{Auth, RateLimiter, SESSION_COOKIE, SessionStore};
use crate::web::frontend;
use crate::web::http::{self, RequestHead};
use crate::web::protocol::{self, WebInputEvent};
use anyhow::{Context, Result};
use ratatui::buffer::Buffer;
use ratatui::layout::Position;
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};
use tungstenite::handshake::derive_accept_key;
use tungstenite::protocol::Role;
use tungstenite::{Message, WebSocket};

/// Reject a request head larger than this (headers only) to bound memory.
const MAX_HEAD_BYTES: usize = 32 * 1024;
/// Cap the request body we read (login form is tiny).
const MAX_BODY_BYTES: usize = 64 * 1024;
/// Give a client this long to send its request head before dropping it.
const HEAD_READ_TIMEOUT: Duration = Duration::from_secs(15);
/// Poll interval for the per-client loop: bounds added output latency while
/// letting the same thread service both socket reads and queued writes.
const WS_POLL_TIMEOUT: Duration = Duration::from_millis(10);
/// Live connections allowed at once. Each one costs a thread, so an unbounded
/// accept loop lets anything that can reach the port exhaust the process.
/// A browser session needs a handful; this leaves room for several of them.
const MAX_CONNECTIONS: usize = 64;

/// State shared between the accept/handler threads. Holds no `App` reference.
struct Shared {
    clients: Mutex<Vec<ClientHandle>>,
    input_tx: Sender<WebInputEvent>,
    auth: Auth,
    sessions: SessionStore,
    limiter: RateLimiter,
    next_id: AtomicU64,
    /// Connections currently held by a handler thread, capped at
    /// [`MAX_CONNECTIONS`]. Owned through [`ConnectionSlot`].
    connections: Arc<AtomicUsize>,
}

/// A claimed connection slot. Releasing it is `Drop`, so every handler exit
/// path — normal return, early error, a panicking thread — frees the slot.
struct ConnectionSlot {
    counter: Arc<AtomicUsize>,
}

impl ConnectionSlot {
    /// Claim a slot, or return `None` when `counter` is already at `cap`.
    fn acquire(counter: &Arc<AtomicUsize>, cap: usize) -> Option<Self> {
        // Claim first and give back on overflow, so two accepts racing at the
        // limit cannot both see room and both proceed.
        let previous = counter.fetch_add(1, Ordering::AcqRel);
        if previous >= cap {
            counter.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(Self {
            counter: Arc::clone(counter),
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A message queued for one client's handler thread to write to its socket.
enum ClientMsg {
    /// Grid dimensions changed (or first frame): tell the browser to resize its
    /// terminal to match before the repaint. Sent as a JSON text frame.
    Resize { cols: u16, rows: u16 },
    /// Encoded ANSI screen bytes. Sent as a binary frame.
    Frame(Vec<u8>),
}

/// A connected browser, as seen by the main loop's broadcast.
struct ClientHandle {
    id: u64,
    /// Screen updates are pushed here; the client's handler thread writes them
    /// to the socket.
    tx: Sender<ClientMsg>,
    /// Set when the client must receive a full repaint on the next frame
    /// (fresh connection or a grid resize).
    needs_full: bool,
}

/// Handle owned by the main loop. Drop stops nothing (threads live until the
/// process exits, which is the intended lifetime), but it is the sole surface
/// the loop uses to move frames out and input in.
pub struct WebServer {
    shared: Arc<Shared>,
    input_rx: Receiver<WebInputEvent>,
    /// Private baseline of the last broadcast buffer, diffed against each frame.
    /// Main-thread only, so it needs no lock.
    baseline: Option<Buffer>,
    /// Cursor sent with the last broadcast. Tracked separately from `baseline`
    /// because the cursor can move on a frame whose cells are all unchanged
    /// (arrow keys in a shell), which would otherwise send nothing.
    baseline_cursor: Option<Position>,
    addr: SocketAddr,
}

impl WebServer {
    /// Bind and start the server from the `[web]` config, building the password
    /// verifier from either `hashed_password` or `password`.
    pub fn start_from_config(web: &crate::config::WebConfig) -> Result<Self> {
        let auth = if let Some(hash) = web.hashed_password.as_deref() {
            Auth::from_hashed(hash)?
        } else if let Some(password) = web.password.as_deref().filter(|p| !p.is_empty()) {
            Auth::from_plaintext(password)?
        } else {
            anyhow::bail!(
                "web server is enabled but no password or hashed_password is configured"
            );
        };
        let bind: IpAddr = web
            .bind
            .parse()
            .with_context(|| format!("web.bind {:?} is not a valid IP address", web.bind))?;
        Self::start(bind, web.port, auth)
    }

    /// Bind the server and start accepting connections in the background.
    fn start(bind: IpAddr, port: u16, auth: Auth) -> Result<Self> {
        let listener = TcpListener::bind((bind, port))
            .with_context(|| format!("binding web server to {bind}:{port}"))?;
        let addr = listener
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::new(bind, port));

        let (input_tx, input_rx) = mpsc::channel();
        let shared = Arc::new(Shared {
            clients: Mutex::new(Vec::new()),
            input_tx,
            auth,
            sessions: SessionStore::new(),
            limiter: RateLimiter::new(),
            next_id: AtomicU64::new(0),
            connections: Arc::new(AtomicUsize::new(0)),
        });

        let accept_shared = Arc::clone(&shared);
        thread::Builder::new()
            .name("nightcrow-web-accept".into())
            .spawn(move || accept_loop(listener, accept_shared))
            .context("spawning web accept thread")?;

        Ok(Self {
            shared,
            input_rx,
            baseline: None,
            baseline_cursor: None,
            addr,
        })
    }

    /// The address the server actually bound (port may differ if 0 was given).
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Drain all input events received from browsers since the last call.
    pub fn drain_input(&self) -> Vec<WebInputEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = self.input_rx.try_recv() {
            out.push(ev);
        }
        out
    }

    /// Send the frame needed to bring every connected client up to `current`.
    ///
    /// New clients (and all clients after a grid resize) get a full repaint;
    /// the rest get the incremental cell diff against the last broadcast. A
    /// no-op when no clients are connected — the baseline is dropped so the
    /// next client to connect receives a full frame.
    ///
    /// `cursor` is the cell `ui::draw` placed the terminal cursor on, which is
    /// not part of `current` and must be replayed explicitly.
    pub fn broadcast(&mut self, current: &Buffer, cursor: Option<Position>) {
        // Lock first and bail on no clients so a disconnected session never
        // pays for frame encoding. The baseline is dropped so the next client
        // to connect is treated as needing a full repaint.
        let mut clients = match self.shared.clients.lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if clients.is_empty() {
            drop(clients);
            self.baseline = None;
            self.baseline_cursor = None;
            return;
        }

        let area_changed = self
            .baseline
            .as_ref()
            .is_some_and(|prev| prev.area() != current.area());
        let cursor_bytes = protocol::encode_cursor(cursor);
        let cursor_moved = cursor != self.baseline_cursor;
        // Incremental diff against our own baseline (a different field from the
        // locked `clients`, so the borrows are disjoint). A frame with no cell
        // changes still ships when the cursor alone moved.
        let update_bytes = if area_changed {
            None
        } else {
            self.baseline
                .as_ref()
                .map(|prev| protocol::encode_update(prev, current))
                .filter(|cells| !cells.is_empty() || cursor_moved)
                .map(|mut bytes| {
                    bytes.extend_from_slice(&cursor_bytes);
                    bytes
                })
        };
        let mut full_bytes: Option<Vec<u8>> = None;

        let area = current.area();
        let (cols, rows) = (area.width, area.height);
        let mut dead = Vec::new();
        for client in clients.iter_mut() {
            let needs_full = client.needs_full || area_changed;
            let result = if needs_full {
                let bytes = full_bytes.get_or_insert_with(|| {
                    let mut bytes = protocol::encode_full_frame(current);
                    bytes.extend_from_slice(&cursor_bytes);
                    bytes
                });
                client.needs_full = false;
                // The browser must resize its terminal to the grid before the
                // repaint lands, so the frame's absolute cursor moves address
                // the right cells.
                client
                    .tx
                    .send(ClientMsg::Resize { cols, rows })
                    .and_then(|()| client.tx.send(ClientMsg::Frame(bytes.clone())))
            } else if let Some(bytes) = update_bytes.as_ref() {
                client.tx.send(ClientMsg::Frame(bytes.clone()))
            } else {
                Ok(())
            };
            if result.is_err() {
                dead.push(client.id);
            }
        }
        if !dead.is_empty() {
            clients.retain(|c| !dead.contains(&c.id));
        }
        drop(clients);
        self.baseline = Some(current.clone());
        self.baseline_cursor = cursor;
    }
}

fn accept_loop(listener: TcpListener, shared: Arc<Shared>) {
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        // Refuse over the cap by closing the socket here rather than writing a
        // 503 from the accept loop: a write to a stalled client would block
        // every other connection behind it.
        let Some(slot) = ConnectionSlot::acquire(&shared.connections, MAX_CONNECTIONS) else {
            tracing::debug!(cap = MAX_CONNECTIONS, "web: refusing connection over cap");
            continue;
        };
        let shared = Arc::clone(&shared);
        // One handler thread per connection. A failed spawn drops the closure,
        // and with it the slot; the accept loop keeps serving others.
        let _ = thread::Builder::new()
            .name("nightcrow-web-conn".into())
            .spawn(move || {
                let _slot = slot;
                handle_connection(stream, shared)
            });
    }
}

fn handle_connection(mut stream: TcpStream, shared: Arc<Shared>) {
    let (head, body) = match read_request(&mut stream) {
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
        if !origin_allowed(&head) {
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

/// Read the request head (up to CRLFCRLF) plus any declared body.
fn read_request(stream: &mut TcpStream) -> Result<(RequestHead, String)> {
    stream.set_read_timeout(Some(HEAD_READ_TIMEOUT)).ok();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() > MAX_HEAD_BYTES {
            anyhow::bail!("request head exceeds {MAX_HEAD_BYTES} bytes");
        }
        let n = stream.read(&mut chunk).context("reading request head")?;
        if n == 0 {
            anyhow::bail!("connection closed before the request head completed");
        }
        buf.extend_from_slice(&chunk[..n]);
    };

    let head_text =
        std::str::from_utf8(&buf[..head_end]).context("request head is not valid UTF-8")?;
    let head = http::parse_request_head(head_text)?;

    let want = head.content_length.min(MAX_BODY_BYTES);
    let mut body = buf[head_end..].to_vec();
    while body.len() < want {
        let n = stream.read(&mut chunk).context("reading request body")?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(want);
    // The WebSocket loop installs its own timeout; clear this one first.
    stream.set_read_timeout(None).ok();
    Ok((head, String::from_utf8_lossy(&body).into_owned()))
}

fn is_authenticated(head: &RequestHead, shared: &Shared) -> bool {
    head.cookie(SESSION_COOKIE)
        .is_some_and(|token| shared.sessions.is_valid(token))
}

/// Whether a WebSocket upgrade's `Origin` is acceptable. Absent Origin (a
/// native client that cannot carry a browser's cookie) is allowed; a present
/// Origin must match the request `Host` authority, else it is a cross-site
/// upgrade and is refused.
fn origin_allowed(head: &RequestHead) -> bool {
    match head.header("origin") {
        None => true,
        Some(origin) => {
            let origin_authority = origin.split_once("://").map(|(_, rest)| rest);
            matches!(
                (origin_authority, head.header("host")),
                (Some(authority), Some(host)) if authority == host
            )
        }
    }
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
        _ => http::html("404 Not Found", "<h1>404 Not Found</h1>"),
    }
}

fn handle_login(body: &str, shared: &Shared) -> Vec<u8> {
    if !shared.limiter.check_and_record(Instant::now()) {
        return http::response(
            "429 Too Many Requests",
            "text/html; charset=utf-8",
            &[],
            frontend::login_page(Some("Too many attempts — wait a minute and try again.")).as_bytes(),
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

/// Complete the WebSocket handshake manually (the request head was already
/// consumed for routing/auth), then run the per-client read/write loop.
fn serve_websocket(mut stream: TcpStream, head: &RequestHead, shared: Arc<Shared>) {
    let Some(key) = head.header("sec-websocket-key") else {
        let _ = stream.write_all(&http::response(
            "400 Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"missing Sec-WebSocket-Key",
        ));
        return;
    };
    let accept = derive_accept_key(key.as_bytes());
    let handshake = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {accept}\r\n\r\n"
    );
    if stream.write_all(handshake.as_bytes()).is_err() {
        return;
    }

    let ws = WebSocket::from_raw_socket(stream, Role::Server, None);
    run_client(ws, shared);
}

fn run_client(mut ws: WebSocket<TcpStream>, shared: Arc<Shared>) {
    let (tx, rx) = mpsc::channel::<ClientMsg>();
    let id = shared.next_id.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut clients) = shared.clients.lock() {
        clients.push(ClientHandle {
            id,
            tx,
            needs_full: true,
        });
    }
    // A read timeout turns the blocking read into a poll so the same thread can
    // also flush queued output frames.
    ws.get_ref().set_read_timeout(Some(WS_POLL_TIMEOUT)).ok();

    loop {
        if !pump_writes(&mut ws, &rx) {
            break;
        }
        if !pump_read(&mut ws, &shared) {
            break;
        }
    }

    if let Ok(mut clients) = shared.clients.lock() {
        clients.retain(|c| c.id != id);
    }
}

/// Drain and send any queued output messages. Returns false on a write error
/// (client gone).
fn pump_writes(ws: &mut WebSocket<TcpStream>, rx: &Receiver<ClientMsg>) -> bool {
    while let Ok(msg) = rx.try_recv() {
        let written = match msg {
            ClientMsg::Resize { cols, rows } => {
                ws.write(Message::text(format!(r#"{{"t":"resize","cols":{cols},"rows":{rows}}}"#)))
            }
            ClientMsg::Frame(bytes) => ws.write(Message::binary(bytes)),
        };
        if written.is_err() {
            return false;
        }
    }
    ws.flush().is_ok()
}

/// Read at most one input message. Returns false when the connection should
/// close; a read timeout (no data yet) returns true so the loop continues.
fn pump_read(ws: &mut WebSocket<TcpStream>, shared: &Shared) -> bool {
    match ws.read() {
        Ok(msg) => {
            if msg.is_close() {
                return false;
            }
            if msg.is_text() || msg.is_binary() {
                let data = msg.into_data();
                if let Ok(text) = std::str::from_utf8(data.as_ref()) {
                    dispatch_input(text, shared);
                }
            }
            true
        }
        Err(tungstenite::Error::Io(e))
            if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) =>
        {
            // Poll timeout: no message this round.
            true
        }
        Err(_) => false,
    }
}

fn dispatch_input(text: &str, shared: &Shared) {
    if protocol::ensure_input_size(text.len()).is_err() {
        tracing::debug!("web: dropping oversized input message");
        return;
    }
    match protocol::decode_input(text) {
        Ok(Some(event)) => {
            let _ = shared.input_tx.send(event);
        }
        Ok(None) => {}
        Err(err) => tracing::debug!(%err, "web: dropping undecodable input"),
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WebConfig;
    use ratatui::layout::Rect;
    use ratatui::style::Style;
    use tungstenite::client::IntoClientRequest;

    #[test]
    fn connection_slot_refuses_over_the_cap() {
        let counter = Arc::new(AtomicUsize::new(0));

        let held: Vec<_> = (0..2)
            .map(|_| ConnectionSlot::acquire(&counter, 2).expect("under the cap"))
            .collect();

        assert!(
            ConnectionSlot::acquire(&counter, 2).is_none(),
            "a third connection must be refused"
        );
        assert_eq!(
            counter.load(Ordering::Acquire),
            2,
            "a refused connection must not leak a slot"
        );
        drop(held);
    }

    #[test]
    fn connection_slot_releases_on_drop() {
        let counter = Arc::new(AtomicUsize::new(0));

        drop(ConnectionSlot::acquire(&counter, 1).expect("under the cap"));

        assert_eq!(counter.load(Ordering::Acquire), 0);
        assert!(
            ConnectionSlot::acquire(&counter, 1).is_some(),
            "the freed slot must be reusable"
        );
    }

    #[test]
    fn find_subsequence_locates_delimiter() {
        assert_eq!(find_subsequence(b"abc\r\n\r\nxyz", b"\r\n\r\n"), Some(3));
        assert_eq!(find_subsequence(b"no delimiter", b"\r\n\r\n"), None);
    }

    fn test_config(password: &str) -> WebConfig {
        WebConfig {
            enabled: true,
            bind: "127.0.0.1".into(),
            // Port 0 asks the OS for a free ephemeral port.
            port: 0,
            password: Some(password.into()),
            hashed_password: None,
        }
    }

    /// Send a raw HTTP request and read the full response (server closes the
    /// connection after each response).
    fn http_request(addr: SocketAddr, raw: &str) -> String {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        stream.write_all(raw.as_bytes()).unwrap();
        let mut buf = Vec::new();
        // Reads until the server closes the socket (Connection: close).
        let _ = stream.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn form_post(body: &str) -> String {
        format!(
            "POST /login HTTP/1.1\r\nHost: x\r\n\
             Content-Type: application/x-www-form-urlencoded\r\n\
             Content-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn session_token(response: &str) -> Option<String> {
        for line in response.lines() {
            if let Some(value) = line.strip_prefix("Set-Cookie: ")
                && let Some(rest) = value.strip_prefix(&format!("{SESSION_COOKIE}="))
            {
                let token = rest.split(';').next()?.trim();
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
        None
    }

    #[test]
    fn login_flow_issues_session_and_gates_the_app_page() {
        let server = WebServer::start_from_config(&test_config("swordfish")).unwrap();
        let addr = server.addr();

        // Unauthenticated GET / serves the login page, not the app.
        let anon = http_request(
            addr,
            "GET / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        );
        assert!(anon.contains("Sign in"), "login page expected");
        assert!(
            !anon.contains("/vendor/xterm.js"),
            "the terminal app must be gated behind auth"
        );

        // Wrong password is rejected.
        let bad = http_request(addr, &form_post("password=nope"));
        assert!(bad.starts_with("HTTP/1.1 401"), "wrong password must 401");

        // Correct password issues a session cookie via a redirect.
        let ok = http_request(addr, &form_post("password=swordfish"));
        assert!(ok.starts_with("HTTP/1.1 303"), "correct password must redirect");
        let token = session_token(&ok).expect("a session cookie");

        // The cookie unlocks the app page.
        let app = http_request(
            addr,
            &format!(
                "GET / HTTP/1.1\r\nHost: x\r\nCookie: {SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(
            app.contains("/vendor/xterm.js"),
            "authenticated GET / serves the terminal app"
        );
    }

    #[test]
    fn serves_vendored_renderer_assets() {
        let server = WebServer::start_from_config(&test_config("pw")).unwrap();
        let addr = server.addr();
        let js = http_request(
            addr,
            "GET /vendor/xterm.js HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
        );
        assert!(js.starts_with("HTTP/1.1 200"));
        assert!(js.contains("application/javascript"));
    }

    #[test]
    fn websocket_requires_auth() {
        let server = WebServer::start_from_config(&test_config("hunter2")).unwrap();
        let addr = server.addr();
        // A WS upgrade without a session cookie is refused before the handshake.
        let resp = http_request(
            addr,
            "GET /ws HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\n\
             Connection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
             Sec-WebSocket-Version: 13\r\nConnection: close\r\n\r\n",
        );
        assert!(resp.starts_with("HTTP/1.1 401"), "unauthenticated WS must 401");
    }

    #[test]
    fn websocket_rejects_cross_origin_even_with_valid_cookie() {
        let server = WebServer::start_from_config(&test_config("hunter2")).unwrap();
        let addr = server.addr();
        let token = session_token(&http_request(addr, &form_post("password=hunter2")))
            .expect("a session cookie");
        // A valid cookie but a foreign Origin (cross-site WebSocket hijack
        // attempt) must be refused before the handshake.
        let resp = http_request(
            addr,
            &format!(
                "GET /ws HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://evil.example\r\n\
                 Upgrade: websocket\r\nConnection: Upgrade\r\n\
                 Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\
                 Cookie: {SESSION_COOKIE}={token}\r\nConnection: close\r\n\r\n"
            ),
        );
        assert!(resp.starts_with("HTTP/1.1 403"), "cross-origin WS must be forbidden");
    }

    #[test]
    fn websocket_mirrors_frame_and_delivers_input() {
        let mut server = WebServer::start_from_config(&test_config("hunter2")).unwrap();
        let addr = server.addr();
        let token = session_token(&http_request(addr, &form_post("password=hunter2")))
            .expect("a session cookie");

        // Open an authenticated WebSocket.
        let stream = TcpStream::connect(addr).unwrap();
        let mut request = format!("ws://{addr}/ws").into_client_request().unwrap();
        request.headers_mut().insert(
            "Cookie",
            format!("{SESSION_COOKIE}={token}").parse().unwrap(),
        );
        let (mut ws, _resp) = tungstenite::client(request, stream).unwrap();
        // Poll for frames without blocking the retry loop below.
        ws.get_ref()
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();

        // Broadcast a frame; retry to absorb the connect-vs-register race. A new
        // client receives a resize control message (text) then the full frame
        // (binary).
        let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 1));
        buffer.set_string(0, 0, "hello", Style::default());
        let mut resize_seen = false;
        let mut frame = None;
        for _ in 0..100 {
            server.broadcast(&buffer, Some(Position::new(2, 0)));
            match ws.read() {
                Ok(msg) if msg.is_text() => {
                    let text = msg.into_text().unwrap();
                    assert!(
                        text.contains("\"t\":\"resize\"")
                            && text.contains("\"cols\":8")
                            && text.contains("\"rows\":1"),
                        "resize control message must carry the grid size, got: {text}"
                    );
                    resize_seen = true;
                }
                Ok(msg) if msg.is_binary() => {
                    frame = Some(msg.into_data());
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => panic!("ws read failed: {e}"),
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(resize_seen, "a new client must receive a resize message first");
        let frame = frame.expect("a broadcast frame within the retry budget");
        assert!(
            frame.windows(5).any(|w| w == b"hello"),
            "the mirrored frame must carry the painted text"
        );
        let cursor_tail = protocol::encode_cursor(Some(Position::new(2, 0)));
        assert!(
            frame.ends_with(&cursor_tail),
            "the frame must end by parking the cursor where the draw left it"
        );

        // Input sent from the browser reaches the main loop's drain.
        ws.write(Message::text(r#"{"t":"key","key":"a"}"#)).unwrap();
        ws.flush().unwrap();
        let mut input = Vec::new();
        for _ in 0..100 {
            input = server.drain_input();
            if !input.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(input.len(), 1, "the keypress must be delivered exactly once");
        assert!(matches!(input[0], WebInputEvent::Key(_)));
    }
}
