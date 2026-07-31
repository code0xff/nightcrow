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
    pub startup_commands: Vec<crate::config::StartupCommand>,
    /// The `--exec` commands already merged into `startup_commands`, remembered
    /// so a config reload can arrive at the same combined list. Empty for every
    /// caller that has none.
    pub cli_startup: Vec<String>,
    pub hot: crate::config::AgentIndicatorConfig,
    pub prefs: PrefsStore,
}

impl ViewerState {
    /// The served repositories, for a transport that needs to reach their
    /// runtimes directly rather than through a route.
    pub fn catalog(&self) -> &crate::web::viewer::catalog::Catalog {
        &self.catalog
    }

    /// Build the served session without binding anything.
    ///
    /// Separate from [`ViewerServer::start`] because the session exists whether
    /// or not a browser listener does: the daemon socket serves this same
    /// state, and a test drives it without taking a port.
    pub fn new(options: ViewerOptions) -> Self {
        Self::with_plugins(options, Vec::new())
    }

    /// Like [`ViewerState::new`], with the `[[plugin]]` table the session's
    /// startup panes may hand themselves to.
    ///
    /// Taken here rather than as a [`ViewerOptions`] field because it has to
    /// reach the catalog before [`crate::web::viewer::catalog::Catalog::set_paths`]
    /// spawns the first hub — a plugin association is decided when a pane is
    /// created, not afterwards.
    pub fn with_plugins(options: ViewerOptions, plugins: Vec<crate::config::PluginConfig>) -> Self {
        let ViewerOptions {
            bind,
            port: _,
            auth,
            repos,
            persist,
            startup_commands,
            cli_startup,
            hot,
            prefs,
        } = options;
        let state = Self {
            catalog: crate::web::viewer::catalog::Catalog::with_startup_plugins_and_exec(
                startup_commands,
                plugins,
                cli_startup,
            ),
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
        };
        state.catalog.set_paths(&repos);
        state
    }
}

impl ViewerServer {
    /// Bind and start from `[web_viewer]`, building the password verifier from
    /// either `hashed_password` or `password`.
    ///
    /// `plugins` is `config.toml`'s `[[plugin]]` table; an empty list means no
    /// pane can be plugin-managed, which is the default.
    #[allow(clippy::too_many_arguments)]
    pub fn start_from_config(
        viewer: &crate::config::WebViewerConfig,
        agent_indicator: &crate::config::AgentIndicatorConfig,
        theme: &crate::config::ThemeConfig,
        paths: &[String],
        persist: bool,
        startup_commands: Vec<crate::config::StartupCommand>,
        cli_startup: Vec<String>,
        plugins: Vec<crate::config::PluginConfig>,
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
        Self::start_with_plugins(
            ViewerOptions {
                bind,
                port: viewer.port,
                auth,
                repos: paths.to_vec(),
                persist,
                startup_commands,
                cli_startup,
                hot: agent_indicator.clone(),
                // The session's accent outlives any one config edit, so `[theme]`
                // only names the colour a session with no stored choice starts in.
                prefs: PrefsStore::load_seeded(theme.preset_index()),
            },
            plugins,
        )
    }

    /// Bind and start accepting. The seeded repositories may be replaced later
    /// through [`ViewerServer::set_repos`].
    pub fn start(options: ViewerOptions) -> Result<Self> {
        Self::start_with_plugins(options, Vec::new())
    }

    /// Like [`ViewerServer::start`], with the `[[plugin]]` table.
    pub fn start_with_plugins(
        options: ViewerOptions,
        plugins: Vec<crate::config::PluginConfig>,
    ) -> Result<Self> {
        // Copied out before the options are consumed: the listener is bound
        // first so a port conflict fails before any repository is opened.
        let (bind, port) = (options.bind, options.port);
        let listener = TcpListener::bind((bind, port))
            .with_context(|| format!("binding viewer server to {bind}:{port}"))?;
        let addr = listener
            .local_addr()
            .unwrap_or_else(|_| SocketAddr::new(bind, port));

        let state = Arc::new(ViewerState::with_plugins(options, plugins));

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

    /// The session this server serves, so another transport can serve the same
    /// one. Shared rather than copied: one session is the whole point.
    pub fn state(&self) -> Arc<ViewerState> {
        Arc::clone(&self.state)
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
