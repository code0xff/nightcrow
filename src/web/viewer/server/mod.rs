//! The viewer's HTTP server: authenticated routes over a shared session.
//!
//! Request handling order is Host/Origin, static assets, authentication,
//! repository lookup, then path validation. Git and I/O details are redacted
//! before responses because they can contain absolute server paths.

mod clone_routes;
mod dispatch;
mod handlers;
mod http_util;
mod mutations;
mod routes;

use crate::session::prefs::PrefsStore;
use crate::session::{SessionOptions, SessionState};
use crate::web::common::auth::{Auth, RateLimiter};
use crate::web::common::sessions;
use crate::web::common::sessions::SessionStore;
use anyhow::{Context, Result};
use std::net::{IpAddr, SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

/// Named for this server so another service on the host cannot authenticate
/// with a cookie issued here.
pub const VIEWER_SESSION_COOKIE: &str = "nightcrow_viewer_session";

pub(super) const SSE_HEARTBEAT: Duration = Duration::from_secs(15);
pub(super) const TERM_POLL_TIMEOUT: Duration = Duration::from_millis(10);

/// HTTP-only state. Repository and terminal ownership lives in SessionState,
/// which the daemon and this transport share.
pub struct ViewerState {
    pub(super) session: Arc<SessionState>,
    pub(super) bound_loopback: bool,
    pub(super) auth: Auth,
    pub(super) sessions: SessionStore,
    pub(super) limiter: RateLimiter,
    pub(super) connections: Arc<AtomicUsize>,
    pub(super) hot: crate::config::AgentIndicatorConfig,
    pub(super) clones: crate::web::viewer::clone_jobs::CloneJobs,
    pub(super) git_available: bool,
}

pub struct ViewerServer {
    state: Arc<ViewerState>,
    addr: SocketAddr,
}

/// Inputs for a server whose authentication has already been constructed.
pub struct ViewerOptions {
    pub bind: IpAddr,
    pub port: u16,
    pub auth: Auth,
    pub sessions: SessionStore,
    pub hot: crate::config::AgentIndicatorConfig,
    pub session: SessionOptions,
}

/// Named configuration boundary used by the daemon startup path.
pub struct ViewerLaunch<'a> {
    pub viewer: &'a crate::config::WebViewerConfig,
    pub agent_indicator: &'a crate::config::AgentIndicatorConfig,
    pub theme: &'a crate::config::ThemeConfig,
    pub shell: &'a crate::config::ShellConfig,
    pub paths: &'a [String],
    pub persist: bool,
    pub startup_commands: Vec<crate::config::StartupCommand>,
    pub cli_startup: Vec<String>,
    pub plugins: Vec<crate::config::PluginConfig>,
}

impl ViewerState {
    pub fn session(&self) -> &SessionState {
        &self.session
    }

    pub fn with_plugins(options: ViewerOptions, plugins: Vec<crate::config::PluginConfig>) -> Self {
        Self {
            bound_loopback: options.bind.is_loopback(),
            auth: options.auth,
            sessions: options.sessions,
            limiter: RateLimiter::new(),
            connections: Arc::new(AtomicUsize::new(0)),
            hot: options.hot,
            clones: Default::default(),
            git_available: crate::git::clone::git_available(),
            session: Arc::new(SessionState::with_plugins(options.session, plugins)),
        }
    }
}

impl ViewerServer {
    /// Build the password verifier, session, listener, and accept thread.
    pub fn start_from_config(launch: ViewerLaunch<'_>) -> Result<Self> {
        let viewer = launch.viewer;
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
        let session_store = sessions::session_store_path()
            .map(SessionStore::load)
            .unwrap_or_else(|err| {
                tracing::warn!(%err, "could not open session store; starting in-memory");
                SessionStore::new()
            });
        Self::start_with_plugins(
            ViewerOptions {
                bind,
                port: viewer.port,
                auth,
                sessions: session_store,
                hot: launch.agent_indicator.clone(),
                session: SessionOptions {
                    repos: launch.paths.to_vec(),
                    persist: launch.persist,
                    startup_commands: launch.startup_commands,
                    cli_startup: launch.cli_startup,
                    shell: launch.shell.clone(),
                    prefs: PrefsStore::load_seeded(launch.theme.preset_index()),
                    status_encoder: crate::web::viewer::status_payload::encode,
                },
            },
            launch.plugins,
        )
    }

    #[cfg(test)]
    pub fn start(options: ViewerOptions) -> Result<Self> {
        Self::start_with_plugins(options, Vec::new())
    }

    pub fn start_with_plugins(
        options: ViewerOptions,
        plugins: Vec<crate::config::PluginConfig>,
    ) -> Result<Self> {
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

    pub fn session_state(&self) -> Arc<SessionState> {
        Arc::clone(&self.state.session)
    }

    pub fn shutdown(&self) {
        self.state.session.shutdown();
    }
}

impl Drop for ViewerServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests;
