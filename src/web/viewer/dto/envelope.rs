use super::status::server_now_millis;
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
    /// sent: the client never needs it, and it is the one field here that says
    /// something about the machine rather than the repository.
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
/// with. The path stays as it is so opening and closing keep their home; this
/// type is where the payload's real job is written down.
///
/// Every field here belongs in `ViewerBootstrap` in `viewer-ui/src/api.ts` too.
/// Renaming or retyping one without doing so fails the fixture contract test;
/// a purely additive field does not, so add it to both while it is in hand.
#[derive(Debug, Clone, Serialize)]
pub struct ViewerBootstrapDto {
    pub repos: Vec<RepoDto>,
    pub hot: HotConfigDto,
    /// Index into the viewer's accent presets, stored server-side so every
    /// device agrees.
    pub accent: usize,
    /// File-sidebar width in CSS px, stored server-side like the accent so
    /// every device opens at the same split.
    pub sidebar_width: u32,
    /// Id of the project a client last selected, so a reload lands there
    /// instead of on the first tab. `None` when nothing has been selected yet
    /// or the remembered project is not currently served — the client then
    /// falls back to the first tab. An id, not the path `prefs.rs` stores:
    /// clients address repositories by id and never learn the path.
    pub active_repo: Option<String>,
    /// This server's wall clock, for dating [`super::ChangedFileDto::mtime`].
    pub now_ms: u64,
    /// Whether this server can clone: false when no `git` is on its PATH, so
    /// the client disables the form instead of starting a job that must fail.
    pub can_clone: bool,
}

impl ViewerBootstrapDto {
    /// Stamps `now_ms` at construction — the value is only useful as "the
    /// server's time when this response was built", so no caller is given the
    /// chance to supply a staler one.
    pub fn new(
        repos: Vec<RepoDto>,
        hot: HotConfigDto,
        accent: usize,
        sidebar_width: u32,
        active_repo: Option<String>,
        can_clone: bool,
    ) -> Self {
        Self {
            repos,
            hot,
            accent,
            sidebar_width,
            active_repo,
            can_clone,
            now_ms: server_now_millis(),
        }
    }
}
