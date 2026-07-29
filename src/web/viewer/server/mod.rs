//! The viewer's HTTP server: read-only git routes plus a live status stream.
//!
//! Runs on its own port with its own session cookie. The cookie is named for
//! this server rather than for nightcrow at large: two servers sharing a cookie
//! name on one host would let a session for one authenticate against the other.
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

mod clone_routes;
mod dispatch;
mod handlers;
mod http_util;
mod mutations;
mod routes;

use crate::web::common::auth::{Auth, RateLimiter, SessionStore};
use crate::web::viewer::prefs::PrefsStore;
use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

/// Named for this server, not for nightcrow: another server on the same host
/// must not be able to authenticate with a session issued here.
pub const VIEWER_SESSION_COOKIE: &str = "nightcrow_viewer_session";

/// How long an idle SSE stream waits before sending a heartbeat. A write is
/// the only way to find out a socket is dead.
pub(super) const SSE_HEARTBEAT: Duration = Duration::from_secs(15);

/// Read timeout on a terminal socket. Bounds how long queued output waits
/// behind a blocked read; terminal latency is felt directly.
pub(super) const TERM_POLL_TIMEOUT: Duration = Duration::from_millis(10);

pub struct ViewerState {
    pub(super) catalog: crate::web::viewer::catalog::Catalog,
    /// Whether the listener is on a loopback address. Gates the Host check:
    /// off-loopback, the operator owns the network path and may front this
    /// with a proxy under any name.
    pub(super) bound_loopback: bool,
    pub(super) auth: Auth,
    pub(super) sessions: SessionStore,
    pub(super) limiter: RateLimiter,
    pub(super) connections: Arc<AtomicUsize>,
    /// Whether catalog changes are mirrored to the shared workspace file. On in
    /// headless `serve` (so opens/closes are remembered), off alongside the TUI
    /// (which owns that file).
    pub(super) persist: bool,
    /// The TUI's recently-touched settings, served to the client so the file
    /// list fades on the same window the TUI does. `auto_follow` is not sent:
    /// it moves the TUI's selection, which the viewer has no analogue for.
    pub(super) hot: crate::config::AgentIndicatorConfig,
    /// Viewer preferences shared by every client (see `prefs.rs`). Always
    /// written: the file is the viewer's own and no TUI owns it.
    pub(super) prefs: PrefsStore,
    /// In-flight and recently finished clones (see `clone_jobs.rs`). A clone
    /// outlives the request that started it, so its result is polled.
    pub(super) clones: crate::web::viewer::clone_jobs::CloneJobs,
    /// Whether `git` was on PATH at startup. Reported to clients so the clone
    /// form is disabled up front rather than failing every job it starts.
    pub(super) git_available: bool,
}

pub struct ViewerServer {
    state: Arc<ViewerState>,
    addr: SocketAddr,
}

/// Everything [`ViewerServer::start`] needs. A struct rather than a parameter
/// list: the server is configured from three unrelated places (`[web_viewer]`,
/// `[agent_indicator]`, the CLI), and positional arguments of the same types
/// were becoming easy to transpose silently.
pub struct ViewerOptions {
    pub bind: IpAddr,
    pub port: u16,
    pub auth: Auth,
    /// Absolute repository paths seeding the catalog; may be empty.
    pub repos: Vec<String>,
    /// Mirror catalog changes into the shared workspace file (headless
    /// `serve` only — alongside the TUI, the TUI owns that file).
    pub persist: bool,
    pub startup_commands: Vec<String>,
    pub hot: crate::config::AgentIndicatorConfig,
    pub prefs: PrefsStore,
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
        Self::start(ViewerOptions {
            bind,
            port: viewer.port,
            auth,
            repos: paths.to_vec(),
            persist,
            startup_commands,
            hot: agent_indicator.clone(),
            prefs: PrefsStore::load(),
        })
    }

    /// Bind and start accepting. The seeded repositories may be replaced later
    /// through [`ViewerServer::set_repos`].
    pub fn start(options: ViewerOptions) -> Result<Self> {
        let ViewerOptions {
            bind,
            port,
            auth,
            repos,
            persist,
            startup_commands,
            hot,
            prefs,
        } = options;
        let listener = TcpListener::bind((bind, port))
            .with_context(|| format!("binding viewer server to {bind}:{port}"))?;
        let addr = listener
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::new(bind, port));

        let state = Arc::new(ViewerState {
            catalog: crate::web::viewer::catalog::Catalog::with_startup(startup_commands),
            bound_loopback: bind.is_loopback(),
            auth,
            sessions: SessionStore::new(),
            limiter: RateLimiter::new(),
            connections: Arc::new(AtomicUsize::new(0)),
            clones: Default::default(),
            git_available: crate::git::clone::git_available(),
            persist,
            hot,
            prefs,
        });
        state.catalog.set_paths(&repos);

        let accept_state = Arc::clone(&state);
        std::thread::Builder::new()
            .name("nightcrow-viewer-accept".into())
            .spawn(move || dispatch::accept_loop(listener, accept_state))
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

#[cfg(test)]
mod tests;
