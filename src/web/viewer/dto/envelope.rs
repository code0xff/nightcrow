use super::status::server_now_millis;
use crate::web::viewer::prefs::ViewerPrefs;
use serde::Serialize;

/// Bumped whenever an existing field changes meaning or disappears. Adding a
/// new optional field does not need a bump.
pub const PROTOCOL_VERSION: u32 = 2;

/// Wrapper carried by every response so a stale client can detect a mismatch.
#[derive(Debug, Serialize)]
pub struct Envelope<T> {
    pub version: u32,
    #[serde(flatten)]
    pub payload: T,
}

impl<T> Envelope<T> {
    pub fn new(payload: T) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            payload,
        }
    }
}

/// One open repository. `id` is opaque and stable for the process lifetime;
/// clients address every other route by it and never by path.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RepoDto {
    pub id: String,
    /// Final path component, for a tab label.
    pub name: String,
    /// Home-relative path for display (`~/code/app`). The absolute path is not
    /// sent: the client never needs it.
    pub display_path: String,
}

/// The part of `[agent_indicator]` the browser can act on. `auto_follow` is
/// omitted: it moves a TUI selection, and the viewer has no analogue.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct HotConfigDto {
    pub enabled: bool,
    pub window_secs: u64,
}

/// What `GET /api/repos` answers: everything the client needs before it can
/// render, in one response.
///
/// Named for what it carries rather than for its route. The route is about
/// repositories — `POST` opens one, `DELETE` closes one — but the `GET` grew
/// into the session's bootstrap, because a client that already polls it every
/// few seconds is the cheapest carrier for anything server-wide it must agree
/// with.
///
/// Every field here belongs in `ViewerBootstrap` in `viewer-ui/src/api.ts` too.
/// Renaming or retyping one without doing so fails the fixture contract test.
#[derive(Debug, Clone, Serialize)]
pub struct ViewerBootstrapDto {
    pub repos: Vec<RepoDto>,
    pub hot: HotConfigDto,
    /// Index into the accent presets. The session's colour, stored server-side
    /// so every device — and every attached TUI — agrees.
    pub accent: usize,
    /// File-sidebar width in CSS px, stored server-side like the accent.
    pub sidebar_width: u32,
    /// Percent of the vertical split given to the diff panel; the terminal
    /// panel takes the rest. Shared between browsers, not shared with the TUI —
    /// see `prefs::ViewerPrefs`.
    pub upper_pct: u32,
    /// Id of the project a client last selected, so a reload lands there
    /// instead of on the first tab. `None` when nothing has been selected yet
    /// or the remembered project is not currently served. An id, not the path
    /// `prefs.rs` stores: clients address repositories by id and never learn
    /// the path.
    pub active_repo: Option<String>,
    /// Which panel each *currently served* project was left maximized in, by
    /// id. Projects with no arrangement are absent, as are remembered ones this
    /// session is not serving.
    pub maximized: std::collections::HashMap<String, &'static str>,
    /// This server's wall clock, for dating [`super::ChangedFileDto::mtime`].
    pub now_ms: u64,
    /// Whether this server can clone: false when no `git` is on its PATH.
    pub can_clone: bool,
}

impl ViewerBootstrapDto {
    /// Stamps `now_ms` at construction — the value is only useful as "the
    /// server's time when this response was built".
    ///
    /// Takes the whole [`ViewerPrefs`] rather than the fields it needs: several
    /// of them are `u32`, and a positional list of those is a pair of arguments
    /// a call site can swap with nothing to catch it. `active_repo` stays
    /// separate because what goes on the wire is the **id** resolved from
    /// `prefs.active_repo`, which only the caller's catalog snapshot can supply.
    /// `maximized` is separate for the same reason.
    pub fn new(
        repos: Vec<RepoDto>,
        hot: HotConfigDto,
        prefs: &ViewerPrefs,
        active_repo: Option<String>,
        maximized: std::collections::HashMap<String, &'static str>,
        can_clone: bool,
    ) -> Self {
        Self {
            repos,
            hot,
            accent: prefs.accent,
            sidebar_width: prefs.sidebar_width,
            upper_pct: prefs.upper_pct,
            active_repo,
            maximized,
            can_clone,
            now_ms: server_now_millis(),
        }
    }
}
