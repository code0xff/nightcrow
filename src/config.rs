use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Upper bound on the number of `[[startup_command]]` + `--exec` panes opened
/// at launch. The value matches the `F3`..`F10` / `<prefix> 3`..`9`,`0` jump-key
/// range, so every startup pane is reachable by a direct key: `F1`/`F2` reach
/// the upper panels (file list, diff) and `F3`..`F10` reach all eight terminal
/// panes. Runtime panes (opened one at a time by `<leader> t`) are not bounded
/// by this — they may exceed eight, in which case the extras past the eighth
/// are reachable by focus cycling (`Shift+←/→`) rather than a jump key.
pub const MAX_STARTUP_COMMANDS: usize = 8;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub layout: LayoutConfig,
    pub log: LogConfig,
    pub theme: ThemeConfig,
    pub agent_indicator: AgentIndicatorConfig,
    pub input: InputConfig,
    pub tree: TreeConfig,
    pub mouse: MouseConfig,
    pub web_mirror: WebMirrorConfig,
    pub web_viewer: WebViewerConfig,
    /// Commands launched in their own terminal pane at startup, in order.
    /// Maps from TOML `[[startup_command]]` array-of-tables. Empty by
    /// default, which preserves the single empty-shell startup behaviour.
    #[serde(rename = "startup_command")]
    pub startup_commands: Vec<StartupCommand>,
}

/// Default leader chord literal. `Ctrl+F` is a one-handed left-hand chord that
/// avoids tmux's own `Ctrl+B` prefix (so nightcrow can run inside tmux) AND the
/// Ctrl chords that an inner Claude Code pane reserves (`Ctrl+G` = external
/// editor, plus `Ctrl+O/R/S/T/L/…`). It also dodges terminal flow control
/// (`Ctrl+Q`/`Ctrl+S` = XON/XOFF) and the shell signals `Ctrl+C/D/Z`. Its only
/// collision is `Ctrl+F` as forward-char (readline) / page-forward (vim), which
/// users almost always reach via the arrow keys / PageDown instead; when needed
/// it stays reachable via `<leader><leader>`.
const DEFAULT_LEADER: &str = "ctrl+f";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseConfig {
    /// Capture the mouse so clicks reach mouse-aware pane programs and wheel
    /// scrolls move the pane under the pointer. While captured, the outer
    /// terminal only performs its own text selection with Shift held — the
    /// standard override every major terminal honors. Set to `false` to give
    /// the mouse back to the outer terminal entirely (plain-drag selection,
    /// no click forwarding).
    pub enabled: bool,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// Default TCP port for the web mirror server.
const DEFAULT_WEB_PORT: u16 = 8090;
/// Viewer default. Adjacent to the mirror's but distinct: both can run at once.
const DEFAULT_WEB_VIEWER_PORT: u16 = 8091;
/// Table name for the mirror's settings. Named for what it is, matching
/// `[web_viewer]`; `[web]` alone did not say which web surface it meant.
const WEB_MIRROR_TABLE: &str = "web_mirror";
const WEB_VIEWER_TABLE: &str = "web_viewer";
/// Default bind address: loopback only. Exposing the server on a routable
/// address is a deliberate opt-in because it grants live control of a shell.
const DEFAULT_WEB_BIND: &str = "127.0.0.1";
/// Length (characters) of an auto-generated web password.
const GENERATED_PASSWORD_LEN: usize = 24;
/// Alphabet for generated passwords: alphanumeric minus visually ambiguous
/// glyphs (0/O, 1/l/I). All chars are TOML-safe, so the persisted value never
/// needs escaping when written as a basic `"..."` string.
const PASSWORD_ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Web mirror server: serve a live, controllable view of this nightcrow over
/// HTTP so a browser and the local terminal drive the same session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebMirrorConfig {
    /// Enable the web mirror. Off by default — turning it on exposes live
    /// view+control of this nightcrow over the network, so it is opt-in.
    pub enabled: bool,
    /// Address to bind. Defaults to loopback (`127.0.0.1`); set to `0.0.0.0`
    /// only deliberately, and prefer an SSH tunnel / reverse proxy for remote
    /// access since the server speaks plain HTTP (no built-in TLS).
    pub bind: String,
    /// TCP port for the web server.
    pub port: u16,
    /// Plaintext login password. When the web server is enabled and neither
    /// this nor `hashed_password` is set, a random password is generated and
    /// written back here so it survives restarts and stays readable.
    pub password: Option<String>,
    /// Optional Argon2 PHC hash — an alternative to storing `password` in
    /// plaintext. Takes precedence over `password` when both are present.
    pub hashed_password: Option<String>,
}

impl Default for WebMirrorConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: DEFAULT_WEB_BIND.to_string(),
            port: DEFAULT_WEB_PORT,
            password: None,
            hashed_password: None,
        }
    }
}

impl WebMirrorConfig {
    /// Whether a login credential is already configured (either form).
    pub fn has_credential(&self) -> bool {
        self.hashed_password.is_some() || self.password.as_deref().is_some_and(|p| !p.is_empty())
    }
}

/// The native web viewer (`[web_viewer]`). Independent of the mirror: its own
/// port, cookie, and credential, so enabling one does not expose the other.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct WebViewerConfig {
    /// Enable the viewer alongside the TUI. Off by default — it exposes both
    /// repository contents and interactive terminals.
    pub enabled: bool,
    /// Address to bind. Loopback by default; the server speaks plain HTTP, so
    /// remote access belongs behind an SSH tunnel or reverse proxy.
    pub bind: String,
    pub port: u16,
    /// Plaintext login password, generated and written back on first enable.
    pub password: Option<String>,
    /// Optional Argon2 PHC hash. Takes precedence over `password`.
    pub hashed_password: Option<String>,
}

impl Default for WebViewerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind: DEFAULT_WEB_BIND.to_string(),
            port: DEFAULT_WEB_VIEWER_PORT,
            password: None,
            hashed_password: None,
        }
    }
}

impl WebViewerConfig {
    pub fn has_credential(&self) -> bool {
        self.hashed_password.is_some() || self.password.as_deref().is_some_and(|p| !p.is_empty())
    }
}

/// Generate a random, human-readable password from the OS RNG.
///
/// Uses a 55-character unambiguous alphabet. The modulo reduction introduces a
/// negligible bias (256 mod 55) that is immaterial for a locally-scoped dev
/// credential; `getrandom` is the same OS entropy source Argon2 salts use.
pub fn generate_password() -> Result<String> {
    let mut bytes = [0u8; GENERATED_PASSWORD_LEN];
    getrandom::fill(&mut bytes)
        .map_err(|e| anyhow::anyhow!("OS RNG unavailable for web password generation: {e}"))?;
    Ok(bytes
        .iter()
        .map(|b| PASSWORD_ALPHABET[usize::from(*b) % PASSWORD_ALPHABET.len()] as char)
        .collect())
}

/// Ensure the enabled web server has a login credential, generating and
/// persisting one when the config has none.
///
/// A no-op when a `password` or `hashed_password` is already set. Otherwise a
/// random password is generated, written back into the config file at `path`
/// (creating it if absent, preserving any existing content and comments), and
/// stored on `cfg` so the running instance uses it. Returns the freshly
/// generated password so the caller can surface it to the user, or `None` when
/// a credential already existed.
pub fn ensure_web_mirror_password(
    cfg: &mut Config,
    path: &std::path::Path,
) -> Result<Option<String>> {
    if cfg.web_mirror.has_credential() {
        return Ok(None);
    }
    let password = generate_password()?;
    persist_password(path, WEB_MIRROR_TABLE, &password)
        .with_context(|| format!("persisting generated web password to {}", path.display()))?;
    cfg.web_mirror.password = Some(password.clone());
    Ok(Some(password))
}

/// Same bootstrap for the viewer's own `[web_viewer]` credential.
///
/// The viewer gets a *separate* password rather than sharing the mirror's: the
/// two servers already run on separate ports with separate cookies, and one
/// credential granting both would make that separation cosmetic.
pub fn ensure_web_viewer_password(
    cfg: &mut Config,
    path: &std::path::Path,
) -> Result<Option<String>> {
    if cfg.web_viewer.has_credential() {
        return Ok(None);
    }
    let password = generate_password()?;
    persist_password(path, WEB_VIEWER_TABLE, &password).with_context(|| {
        format!(
            "persisting generated web viewer password to {}",
            path.display()
        )
    })?;
    cfg.web_viewer.password = Some(password.clone());
    Ok(Some(password))
}

/// Write `password` into the `[{table}]` table of the TOML file at `path`.
///
/// Preserves the rest of the file (including comments) by inserting a single
/// `password = "..."` line: right after an existing `[{table}]` header, or as a
/// new appended table when none exists. The parent directory is created if
/// needed and the file is written user-only (0600 on Unix) since it holds a
/// secret. `password` must contain only TOML-safe characters (the generator's
/// alphabet guarantees this), so it is emitted as a basic string unescaped.
fn persist_password(path: &std::path::Path, table: &str, password: &str) -> Result<()> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?
    } else {
        String::new()
    };
    let updated = insert_password(&existing, table, password);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {}", parent.display()))?;
    }
    std::fs::write(path, &updated)
        .with_context(|| format!("writing config file {}", path.display()))?;
    restrict_permissions(path);
    Ok(())
}

/// Pure text transform behind `persist_web_password`, isolated for testing.
/// Inserts `password = "..."` under the first `[{table}]` header, or appends a
/// new table when the source has none.
fn insert_password(source: &str, table: &str, password: &str) -> String {
    let line = format!("password = \"{password}\"");
    if let Some(insert_at) = table_header_line_end(source, table) {
        let mut out = String::with_capacity(source.len() + line.len() + 1);
        out.push_str(&source[..insert_at]);
        out.push('\n');
        out.push_str(&line);
        out.push_str(&source[insert_at..]);
        out
    } else {
        let mut out = String::with_capacity(source.len() + line.len() + 16);
        out.push_str(source);
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[{table}]\n"));
        out.push_str(&line);
        out.push('\n');
        out
    }
}

/// Byte offset of the end of the first line that is exactly `[{table}]`
/// (ignoring surrounding whitespace), or `None` when no such header exists.
/// Comment lines (`# [web]`) are not headers and are skipped.
fn table_header_line_end(source: &str, table: &str) -> Option<usize> {
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if line.trim() == format!("[{table}]") {
            // Point at the newline (or end of source) that terminates the
            // header line so the insert lands on the following line.
            return Some(offset + line.trim_end_matches('\n').len());
        }
        offset += line.len();
    }
    None
}

/// Best-effort tighten of a secret-bearing file to owner-only permissions.
/// Failure is non-fatal: on platforms without Unix permissions this is a no-op,
/// and a permission error should not stop the server from starting.
fn restrict_permissions(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InputConfig {
    /// The leader (prefix) chord. Every nightcrow app command is reached by
    /// pressing this key, then a follow-up key (tmux-style). Accepts a single
    /// `ctrl+<ascii>` chord; the parser rejects anything that doubles as a
    /// no-prefix reserved key (F1..F10, Shift+arrows, Shift+PgUp/PgDn).
    pub leader: String,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            leader: DEFAULT_LEADER.to_string(),
        }
    }
}

/// Parse a leader chord string (e.g. `"ctrl+b"`) into a `KeyEvent`.
///
/// Only `ctrl+<ascii-printable>` chords are accepted. The chord must be a key
/// that `encode_key` can turn into literal bytes (so `<L><L>` can pass the
/// leader through to the PTY) and must NOT collide with a no-prefix reserved
/// key. F-keys, Shift+arrows, and Shift+PgUp/PgDn are reserved and rejected.
pub fn parse_leader(spec: &str) -> Result<KeyEvent> {
    let normalized = spec.trim().to_ascii_lowercase();
    let rest = normalized.strip_prefix("ctrl+").ok_or_else(|| {
        anyhow::anyhow!(
            "input.leader \"{spec}\" must be a ctrl chord like \"ctrl+b\" \
             (only ctrl+<letter> leaders are supported)"
        )
    })?;
    let mut chars = rest.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        anyhow::ensure!(
            false,
            "input.leader \"{spec}\" must name exactly one ascii character after ctrl+"
        );
        unreachable!()
    };
    anyhow::ensure!(
        c.is_ascii_alphabetic(),
        "input.leader \"{spec}\" must use an ascii letter after ctrl+ \
         (e.g. ctrl+b; ctrl+1, ctrl+-, ctrl+space are not allowed)"
    );
    // Terminals send Ctrl+I as Tab (0x09) and Ctrl+M as Enter/CR (0x0d), so
    // crossterm surfaces those as KeyCode::Tab / KeyCode::Enter — never the
    // Char('i')/Char('m') + CONTROL event that is_leader_key looks for. Such a
    // leader could be armed but never recognized, so reject it up front.
    anyhow::ensure!(
        !matches!(c, 'i' | 'm'),
        "input.leader \"{spec}\" is not usable: terminals deliver Ctrl+I as Tab \
         and Ctrl+M as Enter, so this leader would never be recognized"
    );
    // Restricting to letters guarantees `<L><L>` literal pass-through works:
    // `encode_key` maps Ctrl+A..Ctrl+Z to control bytes 1..26 via the xterm
    // convention. Digits and punctuation (e.g. ctrl+1) have no single-control-
    // byte encoding, so encode_key would send the literal char instead and the
    // pass-through would break — hence they are rejected above.
    Ok(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

/// A single reserved startup command. `name` labels the pane's tab; when
/// absent the command text is used as the label.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartupCommand {
    /// Optional tab label. Falls back to `command` when omitted.
    pub name: Option<String>,
    /// Shell command run in the pane immediately on launch.
    pub command: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Yellow,
    Cyan,
    Green,
    Magenta,
    Blue,
}

// Compile-time guard: a future refactor must not shrink `Accent::ALL` to
// empty. `from_index` would otherwise rely on a runtime fallback we'd
// rather not exercise. `const` items don't accept `_` inside an `impl`
// block, so this lives at module scope.
const _: () = assert!(!Accent::ALL.is_empty(), "Accent::ALL must be non-empty");

impl Accent {
    // Variant declaration order MUST match this slice so accent_idx values
    // persisted in pre-existing session.json files keep mapping to the same
    // color after the strong-enum migration.
    pub const ALL: &'static [Accent] = &[
        Accent::Yellow,
        Accent::Cyan,
        Accent::Green,
        Accent::Magenta,
        Accent::Blue,
    ];

    pub fn color(self) -> ratatui::style::Color {
        use ratatui::style::Color::*;
        match self {
            Accent::Yellow => Yellow,
            Accent::Green => Green,
            Accent::Cyan => Cyan,
            Accent::Magenta => Magenta,
            Accent::Blue => Blue,
        }
    }

    pub fn index(self) -> usize {
        // Fall back to 0 when a variant is missing from `ALL` — should be
        // unreachable, but a runtime panic on a UI helper is worse than a
        // silently miscoloured tile. The roundtrip test pins the invariant.
        Self::ALL.iter().position(|&a| a == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Accent {
        // The compile-time guard above keeps `len > 0`, so `% len` is sound.
        // `get(...).copied()` is the same value as direct indexing here; the
        // form matches the explicit non-panicking pattern used for `index`.
        Self::ALL
            .get(idx % Self::ALL.len())
            .copied()
            .unwrap_or(Accent::Yellow)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Accent color preset.
    pub name: Accent,
}

impl ThemeConfig {
    pub fn preset_index(&self) -> usize {
        self.name.index()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogRotation {
    #[default]
    Daily,
    Hourly,
    Size,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    /// Enable file-based logging
    pub enabled: bool,
    /// Log directory — relative paths are resolved from the repo root
    pub dir: String,
    /// Rotation policy
    pub rotation: LogRotation,
    /// Maximum file size in MB before rotating (used when rotation = Size)
    pub max_size_mb: u64,
    /// Delete log files older than this many days (0 = keep forever)
    pub max_days: u32,
    /// Opt-in: record terminal prompt input line by line
    pub prompt_log: bool,
    /// Minimum log level
    pub level: LogLevel,
    /// Number of commits loaded per commit-log page. Must lie in 50..=500.
    /// The default (100) is the sweet spot for the async refresh path: small
    /// enough that the background worker returns in well under a frame, big
    /// enough that scrolling rarely outruns the prefetch threshold.
    pub commit_log_page_size: usize,
    /// Trigger a background prefetch once the selection is within this many
    /// rows of the loaded tail. Must be in 1..=page_size.
    pub commit_log_prefetch_threshold: usize,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: ".nightcrow/logs".to_string(),
            rotation: LogRotation::default(),
            max_size_mb: 10,
            max_days: 7,
            prompt_log: false,
            level: LogLevel::default(),
            commit_log_page_size: 100,
            commit_log_prefetch_threshold: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Percentage of vertical space for the upper (diff) panel (1–99)
    pub upper_pct: u16,
    /// Percentage of horizontal space for the file list within the upper panel (1–99)
    pub file_list_pct: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            upper_pct: 55,
            file_list_pct: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentIndicatorConfig {
    /// Show the "recently touched" marker next to files in the status panel.
    pub enabled: bool,
    /// Seconds within which a file is considered hot after its mtime.
    /// Must be >= 3 so the bright→normal fade transition stays observable.
    pub hot_window_secs: u64,
    /// When idle (no manual navigation for >=2s), move selection to the
    /// freshest hot file. Opt-in: set to `true` so the file list follows
    /// whichever file was most recently touched on disk — useful when an
    /// agent, build script, or external process is editing files in a
    /// neighbouring pane.
    pub auto_follow: bool,
}

impl Default for AgentIndicatorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hot_window_secs: 15,
            auto_follow: false,
        }
    }
}

/// Configuration for the read-only file-tree navigator (`ViewMode::Tree`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeConfig {
    /// Hide paths matched by `.gitignore` (e.g. `target/`, `node_modules/`).
    /// On by default so the tree doesn't explode into build artifacts; set to
    /// `false` to browse every file on disk.
    pub respect_gitignore: bool,
    /// Maximum directory depth the navigator will expand into. A guard against
    /// pathologically deep trees; expansion past this depth is a no-op. Must be
    /// in 1..=1024.
    pub max_depth: usize,
    /// Watch expanded directories for filesystem changes and refresh the tree
    /// live while Tree mode is open. On by default; only the visible (expanded)
    /// directories are watched, non-recursively. Set to `false` to fall back to
    /// refreshing only on Tree-mode entry — useful on very large trees or
    /// filesystems where native watching is costly or unsupported.
    pub live_watch: bool,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            max_depth: 64,
            live_watch: true,
        }
    }
}

fn default_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".nightcrow").join("config.toml"))
}

/// The path nightcrow reads/writes its config at (`~/.nightcrow/config.toml`),
/// resolved regardless of whether the file exists yet. Errors only when the
/// home directory cannot be determined. Used by the web-password bootstrap,
/// which may need to create the file to persist a generated credential.
pub fn config_file_path() -> Result<PathBuf> {
    default_config_path()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory for config path"))
}

/// The shipped, commented configuration template, embedded at compile time so
/// a standalone binary (with no source checkout alongside it) can still hand
/// the user a starting file. `nightcrow init` writes this verbatim, and
/// `example_config_parses_and_validates` guards that it always parses and
/// validates against the current `Config`.
pub const EXAMPLE_CONFIG: &str = include_str!("../config.example.toml");

/// Result of `init_config`, so the caller can report precisely which path was
/// touched and whether anything was written.
pub enum InitOutcome {
    Created(PathBuf),
    AlreadyExists(PathBuf),
}

/// Write the embedded template to `~/.nightcrow/config.toml`, creating the
/// parent directory if needed. An existing file is preserved unless `force`
/// is set, so re-running `init` never clobbers a user's edits by accident.
pub fn init_config(force: bool) -> Result<InitOutcome> {
    let path = default_config_path()
        .ok_or_else(|| anyhow::anyhow!("cannot determine home directory for config path"))?;
    write_config_template(&path, force)
}

/// Path-explicit core of `init_config` (no `$HOME` lookup) so the write/skip
/// behaviour is unit-testable against a temp directory.
fn write_config_template(path: &std::path::Path, force: bool) -> Result<InitOutcome> {
    if path.exists() && !force {
        return Ok(InitOutcome::AlreadyExists(path.to_path_buf()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating config directory {}", parent.display()))?;
    }
    std::fs::write(path, EXAMPLE_CONFIG)
        .with_context(|| format!("writing config file {}", path.display()))?;
    Ok(InitOutcome::Created(path.to_path_buf()))
}

pub fn load_config() -> Result<Config> {
    let path = match default_config_path() {
        Some(p) if p.exists() => p,
        _ => return Ok(Config::default()),
    };

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading config file {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&text).with_context(|| format!("parsing config file {}", path.display()))?;
    validate_config(&cfg)?;
    Ok(cfg)
}

fn validate_config(cfg: &Config) -> Result<()> {
    anyhow::ensure!(
        cfg.layout.upper_pct >= 1 && cfg.layout.upper_pct <= 99,
        "layout.upper_pct must be between 1 and 99"
    );
    anyhow::ensure!(
        cfg.layout.file_list_pct >= 1 && cfg.layout.file_list_pct <= 99,
        "layout.file_list_pct must be between 1 and 99"
    );
    anyhow::ensure!(
        cfg.agent_indicator.hot_window_secs >= 3 && cfg.agent_indicator.hot_window_secs <= 3600,
        "agent_indicator.hot_window_secs must be between 3 and 3600"
    );
    anyhow::ensure!(
        (50..=500).contains(&cfg.log.commit_log_page_size),
        "log.commit_log_page_size must be between 50 and 500"
    );
    anyhow::ensure!(
        cfg.log.commit_log_prefetch_threshold >= 1
            && cfg.log.commit_log_prefetch_threshold <= cfg.log.commit_log_page_size,
        "log.commit_log_prefetch_threshold must be between 1 and log.commit_log_page_size"
    );
    // `max_size_mb == 0` would make SizeRollingAppender rotate on every
    // write (and even degenerate to creating a new file per write call),
    // so disallow it. The upper bound is a sanity ceiling that still
    // allows hours of trace logging at high volume.
    anyhow::ensure!(
        (1..=10_000).contains(&cfg.log.max_size_mb),
        "log.max_size_mb must be between 1 and 10000"
    );
    // `max_days == 0` is the documented "keep forever" sentinel and is
    // intentionally accepted; only the upper bound is sanity-checked so a
    // typo in years-vs-days doesn't silently produce log retention that
    // exceeds the host's life.
    anyhow::ensure!(
        cfg.log.max_days <= 3650,
        "log.max_days must be at most 3650 (10 years); 0 = keep forever"
    );
    anyhow::ensure!(
        cfg.startup_commands.len() <= MAX_STARTUP_COMMANDS,
        "at most {MAX_STARTUP_COMMANDS} [[startup_command]] entries are allowed, found {}",
        cfg.startup_commands.len()
    );
    for (i, sc) in cfg.startup_commands.iter().enumerate() {
        anyhow::ensure!(
            !sc.command.trim().is_empty(),
            "startup_command[{i}].command must not be empty"
        );
    }
    anyhow::ensure!(
        (1..=1024).contains(&cfg.tree.max_depth),
        "tree.max_depth must be between 1 and 1024"
    );
    // The web server only needs a valid bind/port when it is enabled; a
    // disabled section is never acted on, so leave its fields unchecked.
    if cfg.web_mirror.enabled {
        anyhow::ensure!(
            cfg.web_mirror.port != 0,
            "web_mirror.port must be non-zero when web_mirror.enabled"
        );
        cfg.web_mirror
            .bind
            .parse::<std::net::IpAddr>()
            .with_context(|| {
                format!(
                    "web_mirror.bind \"{}\" is not a valid IP address",
                    cfg.web_mirror.bind
                )
            })?;
    }
    // Surface a bad leader at startup (plain stderr) rather than letting the
    // app fall back to a silent default the user did not ask for.
    parse_leader(&cfg.input.leader)?;
    Ok(())
}

/// Merge config `[[startup_command]]` entries with CLI `--exec` commands into
/// the final ordered list of panes to open at launch. Config entries come
/// first, then CLI commands (labelled by their command text). The combined
/// count is held to `MAX_STARTUP_COMMANDS`, and empty `--exec` values are
/// rejected — config entries were already validated by `validate_config`.
pub fn resolve_startup_commands(cfg: &Config, cli_exec: &[String]) -> Result<Vec<StartupCommand>> {
    let mut resolved = cfg.startup_commands.clone();
    for (i, command) in cli_exec.iter().enumerate() {
        anyhow::ensure!(
            !command.trim().is_empty(),
            "--exec[{i}] command must not be empty"
        );
        resolved.push(StartupCommand {
            name: None,
            command: command.clone(),
        });
    }
    anyhow::ensure!(
        resolved.len() <= MAX_STARTUP_COMMANDS,
        "at most {MAX_STARTUP_COMMANDS} startup panes are allowed \
         (config [[startup_command]] + --exec combined), found {}",
        resolved.len()
    );
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        validate_config(&Config::default()).unwrap();
    }

    #[test]
    fn example_config_parses_and_validates() {
        // Guards the shipped config.example.toml against drift: it must parse
        // into Config and pass the same validation as a real user file. This is
        // the exact text `nightcrow init` writes, so the guard covers both.
        let cfg: Config = toml::from_str(EXAMPLE_CONFIG).expect("config.example.toml should parse");
        validate_config(&cfg).expect("config.example.toml should validate");
    }

    #[test]
    fn write_config_template_creates_file_and_parent_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        match write_config_template(&path, false).unwrap() {
            InitOutcome::Created(p) => assert_eq!(p, path),
            InitOutcome::AlreadyExists(_) => panic!("expected Created on a fresh path"),
        }
        let written = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written, EXAMPLE_CONFIG);
    }

    #[test]
    fn write_config_template_preserves_existing_without_force() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "# user edits\n").unwrap();
        match write_config_template(&path, false).unwrap() {
            InitOutcome::AlreadyExists(p) => assert_eq!(p, path),
            InitOutcome::Created(_) => panic!("must not overwrite an existing file"),
        }
        // The user's content survives untouched.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# user edits\n");
    }

    #[test]
    fn write_config_template_overwrites_with_force() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "# stale\n").unwrap();
        match write_config_template(&path, true).unwrap() {
            InitOutcome::Created(p) => assert_eq!(p, path),
            InitOutcome::AlreadyExists(_) => panic!("force should rewrite the file"),
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), EXAMPLE_CONFIG);
    }

    #[test]
    fn parse_toml_overrides() {
        let toml = r#"
[layout]
upper_pct = 60
file_list_pct = 30
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.layout.upper_pct, 60);
        assert_eq!(cfg.layout.file_list_pct, 30);
    }

    #[test]
    fn validation_rejects_out_of_range() {
        let mut cfg = Config::default();
        cfg.layout.upper_pct = 0;
        assert!(validate_config(&cfg).is_err());
        cfg.layout.upper_pct = 100;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn parse_rejects_invalid_log_rotation() {
        let toml = r#"
[log]
rotation = "weekly"
"#;
        assert!(toml::from_str::<Config>(toml).is_err());
    }

    #[test]
    fn parse_rejects_invalid_log_level() {
        let toml = r#"
[log]
level = "verbose"
"#;
        assert!(toml::from_str::<Config>(toml).is_err());
    }

    #[test]
    fn parse_accepts_all_valid_rotations() {
        for rotation in &["daily", "hourly", "size"] {
            let toml = format!("[log]\nrotation = \"{rotation}\"\n");
            assert!(
                toml::from_str::<Config>(&toml).is_ok(),
                "rotation={rotation} should parse"
            );
        }
    }

    #[test]
    fn parse_accepts_all_valid_levels() {
        for level in &["error", "warn", "info", "debug", "trace"] {
            let toml = format!("[log]\nlevel = \"{level}\"\n");
            assert!(
                toml::from_str::<Config>(&toml).is_ok(),
                "level={level} should parse"
            );
        }
    }

    #[test]
    fn log_config_defaults_are_sane() {
        let cfg = LogConfig::default();
        assert!(cfg.enabled);
        assert!(!cfg.prompt_log);
        assert_eq!(cfg.rotation, LogRotation::Daily);
        assert_eq!(cfg.level, LogLevel::Info);
        assert_eq!(cfg.max_days, 7);
        assert_eq!(cfg.commit_log_page_size, 100);
        assert_eq!(cfg.commit_log_prefetch_threshold, 25);
    }

    #[test]
    fn commit_log_pagination_parses_from_toml() {
        let toml = r#"
[log]
commit_log_page_size = 400
commit_log_prefetch_threshold = 80
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.log.commit_log_page_size, 400);
        assert_eq!(cfg.log.commit_log_prefetch_threshold, 80);
        validate_config(&cfg).unwrap();
    }

    #[test]
    fn commit_log_page_size_validation_rejects_out_of_range() {
        let mut cfg = Config::default();
        cfg.log.commit_log_page_size = 49;
        assert!(validate_config(&cfg).is_err());
        cfg.log.commit_log_page_size = 501;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn commit_log_prefetch_threshold_validation_rejects_zero() {
        let mut cfg = Config::default();
        cfg.log.commit_log_prefetch_threshold = 0;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn commit_log_prefetch_threshold_validation_rejects_above_page_size() {
        let mut cfg = Config::default();
        cfg.log.commit_log_page_size = 300;
        cfg.log.commit_log_prefetch_threshold = 301;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn theme_default_matches_documented_preset() {
        let cfg = ThemeConfig::default();

        assert_eq!(cfg.name, Accent::Yellow);
        assert_eq!(cfg.preset_index(), 0);
    }

    #[test]
    fn accent_index_from_index_roundtrip_for_every_variant() {
        // Pin the ALL slice against the enum: a missing entry would make
        // `index()` return 0 silently, miscolouring a real variant as the
        // default. Iterate every variant via a match so a future variant
        // addition forces this test to be updated.
        let all = [
            Accent::Yellow,
            Accent::Cyan,
            Accent::Green,
            Accent::Magenta,
            Accent::Blue,
        ];
        for a in all {
            let idx = a.index();
            assert!(idx < Accent::ALL.len(), "{a:?} index {idx} out of range");
            assert_eq!(Accent::from_index(idx), a, "roundtrip failed for {a:?}");
        }
        // And confirm the canonical slice length stays in sync.
        assert_eq!(Accent::ALL.len(), all.len());
    }

    #[test]
    fn log_max_size_mb_validation_rejects_zero_and_huge() {
        let mut cfg = Config::default();
        cfg.log.max_size_mb = 0;
        assert!(validate_config(&cfg).is_err());
        cfg.log.max_size_mb = 10_001;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn log_max_size_mb_validation_accepts_in_range() {
        let mut cfg = Config::default();
        cfg.log.max_size_mb = 1;
        assert!(validate_config(&cfg).is_ok());
        cfg.log.max_size_mb = 10_000;
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn log_max_days_validation_accepts_zero_as_keep_forever_sentinel() {
        let mut cfg = Config::default();
        cfg.log.max_days = 0;
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn log_max_days_validation_rejects_unreasonable_horizon() {
        let mut cfg = Config::default();
        cfg.log.max_days = 3651;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn accent_from_index_wraps_out_of_range() {
        // Defensive: a stale session.json with a huge accent_idx must not
        // panic — `from_index` wraps via `%`. The compile-time guard above
        // keeps `ALL` non-empty so `% len` is sound.
        assert_eq!(
            Accent::from_index(usize::MAX),
            Accent::from_index(usize::MAX % Accent::ALL.len())
        );
        assert_eq!(Accent::from_index(Accent::ALL.len()), Accent::from_index(0));
    }

    #[test]
    fn agent_indicator_defaults_are_sane() {
        let cfg = AgentIndicatorConfig::default();
        assert!(cfg.enabled);
        assert!(!cfg.auto_follow);
        assert_eq!(cfg.hot_window_secs, 15);
    }

    #[test]
    fn agent_indicator_parses_from_toml() {
        let toml = r#"
[agent_indicator]
enabled = false
hot_window_secs = 30
auto_follow = false
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(!cfg.agent_indicator.enabled);
        assert!(!cfg.agent_indicator.auto_follow);
        assert_eq!(cfg.agent_indicator.hot_window_secs, 30);
    }

    #[test]
    fn mouse_capture_defaults_on_and_parses_from_toml() {
        assert!(Config::default().mouse.enabled);

        let cfg: Config = toml::from_str("[mouse]\nenabled = false\n").unwrap();
        assert!(!cfg.mouse.enabled);
    }

    #[test]
    fn agent_indicator_validation_rejects_too_small_window() {
        let mut cfg = Config::default();
        cfg.agent_indicator.hot_window_secs = 2;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn agent_indicator_validation_rejects_too_large_window() {
        let mut cfg = Config::default();
        cfg.agent_indicator.hot_window_secs = 3601;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn startup_commands_default_to_empty() {
        let cfg = Config::default();
        assert!(cfg.startup_commands.is_empty());
        // A config without the table also defaults to empty and validates.
        let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
        assert!(cfg.startup_commands.is_empty());
        validate_config(&cfg).unwrap();
    }

    #[test]
    fn startup_commands_parse_array_of_tables() {
        let toml = r#"
[[startup_command]]
name = "Claude"
command = "claude"

[[startup_command]]
command = "cargo test"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.startup_commands.len(), 2);
        assert_eq!(cfg.startup_commands[0].name.as_deref(), Some("Claude"));
        assert_eq!(cfg.startup_commands[0].command, "claude");
        assert_eq!(cfg.startup_commands[1].name, None);
        assert_eq!(cfg.startup_commands[1].command, "cargo test");
        validate_config(&cfg).unwrap();
    }

    #[test]
    fn startup_command_validation_rejects_empty_command() {
        let mut cfg = Config::default();
        cfg.startup_commands.push(StartupCommand {
            name: Some("blank".into()),
            command: "   ".into(),
        });
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn resolve_startup_commands_appends_cli_exec_after_config() {
        let mut cfg = Config::default();
        cfg.startup_commands.push(StartupCommand {
            name: Some("Claude".into()),
            command: "claude".into(),
        });
        let resolved =
            resolve_startup_commands(&cfg, &["codex".to_string(), "vim".to_string()]).unwrap();
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].command, "claude");
        assert_eq!(resolved[0].name.as_deref(), Some("Claude"));
        // CLI entries carry no name and are ordered after config entries.
        assert_eq!(resolved[1].command, "codex");
        assert_eq!(resolved[1].name, None);
        assert_eq!(resolved[2].command, "vim");
    }

    #[test]
    fn resolve_startup_commands_empty_when_nothing_configured() {
        let resolved = resolve_startup_commands(&Config::default(), &[]).unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn resolve_startup_commands_rejects_empty_exec() {
        let resolved = resolve_startup_commands(&Config::default(), &["  ".to_string()]);
        assert!(resolved.is_err());
    }

    #[test]
    fn resolve_startup_commands_caps_combined_total() {
        let mut cfg = Config::default();
        for i in 0..4 {
            cfg.startup_commands.push(StartupCommand {
                name: None,
                command: format!("echo {i}"),
            });
        }
        // 4 config + 5 CLI = 9 > MAX_STARTUP_COMMANDS (8).
        let cli: Vec<String> = (0..5).map(|i| format!("run {i}")).collect();
        assert!(resolve_startup_commands(&cfg, &cli).is_err());
        // 4 config + 4 CLI = 8 is exactly the cap.
        let cli: Vec<String> = (0..4).map(|i| format!("run {i}")).collect();
        assert_eq!(
            resolve_startup_commands(&cfg, &cli).unwrap().len(),
            MAX_STARTUP_COMMANDS
        );
    }

    #[test]
    fn startup_command_validation_rejects_too_many() {
        let mut cfg = Config::default();
        for i in 0..(MAX_STARTUP_COMMANDS + 1) {
            cfg.startup_commands.push(StartupCommand {
                name: None,
                command: format!("echo {i}"),
            });
        }
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn startup_command_validation_accepts_max() {
        let mut cfg = Config::default();
        for i in 0..MAX_STARTUP_COMMANDS {
            cfg.startup_commands.push(StartupCommand {
                name: None,
                command: format!("echo {i}"),
            });
        }
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn input_leader_defaults_to_ctrl_f() {
        let cfg = Config::default();
        assert_eq!(cfg.input.leader, "ctrl+f");
        let leader = parse_leader(&cfg.input.leader).unwrap();
        assert_eq!(leader.code, KeyCode::Char('f'));
        assert!(leader.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn parse_leader_rejects_unencodable_ctrl_chords() {
        // Digits and punctuation have no single control-byte encoding, so they
        // would break `<L><L>` literal pass-through and must be rejected.
        for spec in ["ctrl+1", "ctrl+-", "ctrl+/", "ctrl+@"] {
            assert!(
                parse_leader(spec).is_err(),
                "{spec} must be rejected as a leader"
            );
        }
    }

    #[test]
    fn parse_leader_rejects_terminal_alias_chords() {
        // Ctrl+I == Tab and Ctrl+M == Enter at the byte level, so crossterm
        // never reports them as Char('i')/Char('m') and the leader would be
        // unrecognizable.
        assert!(parse_leader("ctrl+i").is_err(), "ctrl+i aliases Tab");
        assert!(parse_leader("ctrl+m").is_err(), "ctrl+m aliases Enter");
        // Neighboring letters remain valid.
        assert!(parse_leader("ctrl+j").is_ok());
        assert!(parse_leader("ctrl+n").is_ok());
    }

    #[test]
    fn parse_leader_rejects_non_ctrl_and_multichar() {
        assert!(parse_leader("g").is_err(), "bare key is not a ctrl chord");
        assert!(parse_leader("ctrl+ab").is_err(), "leader is a single key");
    }

    #[test]
    fn input_leader_parses_from_toml() {
        let toml = r#"
[input]
leader = "ctrl+a"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.input.leader, "ctrl+a");
        validate_config(&cfg).unwrap();
        let leader = parse_leader(&cfg.input.leader).unwrap();
        assert_eq!(leader.code, KeyCode::Char('a'));
    }

    #[test]
    fn parse_leader_accepts_uppercase_and_whitespace() {
        let leader = parse_leader("  CTRL+B  ").unwrap();
        assert_eq!(leader.code, KeyCode::Char('b'));
        assert!(leader.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn parse_leader_rejects_non_ctrl_chords() {
        assert!(parse_leader("b").is_err());
        assert!(parse_leader("alt+b").is_err());
        assert!(parse_leader("shift+b").is_err());
    }

    #[test]
    fn parse_leader_rejects_reserved_and_multichar_keys() {
        // F-keys, named keys, and multi-char specs are not single ctrl+ascii
        // chords, so they fail the ctrl+ prefix / single-char gates.
        assert!(parse_leader("ctrl+f1").is_err());
        assert!(parse_leader("f1").is_err());
        assert!(parse_leader("ctrl+pageup").is_err());
        assert!(parse_leader("ctrl+").is_err());
        assert!(parse_leader("ctrl+ ").is_err());
    }

    #[test]
    fn validate_rejects_bad_leader() {
        let mut cfg = Config::default();
        cfg.input.leader = "f1".to_string();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn tree_config_defaults_are_sane() {
        let cfg = TreeConfig::default();
        assert!(cfg.respect_gitignore);
        assert_eq!(cfg.max_depth, 64);
        assert!(cfg.live_watch, "live watching is on by default");
    }

    #[test]
    fn tree_config_parses_from_toml() {
        let toml = r#"
[tree]
respect_gitignore = false
max_depth = 12
live_watch = false
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(!cfg.tree.respect_gitignore);
        assert_eq!(cfg.tree.max_depth, 12);
        assert!(!cfg.tree.live_watch);
        validate_config(&cfg).unwrap();
    }

    #[test]
    fn tree_max_depth_validation_rejects_out_of_range() {
        let mut cfg = Config::default();
        cfg.tree.max_depth = 0;
        assert!(validate_config(&cfg).is_err());
        cfg.tree.max_depth = 1025;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn config_without_tree_table_defaults() {
        // A pre-existing config file with no [tree] table must still parse and
        // validate, falling back to defaults.
        let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
        assert!(cfg.tree.respect_gitignore);
        assert_eq!(cfg.tree.max_depth, 64);
        assert!(cfg.tree.live_watch);
        validate_config(&cfg).unwrap();
    }

    #[test]
    fn web_config_defaults_are_off_and_loopback() {
        let cfg = WebMirrorConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.bind, "127.0.0.1");
        assert_eq!(cfg.port, 8090);
        assert!(cfg.password.is_none());
        assert!(cfg.hashed_password.is_none());
        assert!(!cfg.has_credential());
    }

    #[test]
    fn web_mirror_config_parses_from_toml() {
        let toml = r#"
[web_mirror]
enabled = true
bind = "0.0.0.0"
port = 9000
password = "hunter2"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.web_mirror.enabled);
        assert_eq!(cfg.web_mirror.bind, "0.0.0.0");
        assert_eq!(cfg.web_mirror.port, 9000);
        assert_eq!(cfg.web_mirror.password.as_deref(), Some("hunter2"));
        assert!(cfg.web_mirror.has_credential());
        validate_config(&cfg).unwrap();
    }

    #[test]
    fn config_without_web_table_defaults() {
        // A pre-existing config file with no [web_mirror] table must still parse and
        // validate, falling back to the disabled default.
        let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
        assert!(!cfg.web_mirror.enabled);
        assert_eq!(cfg.web_mirror.port, 8090);
        validate_config(&cfg).unwrap();
    }

    #[test]
    fn web_has_credential_treats_empty_password_as_missing() {
        let empty = WebMirrorConfig {
            password: Some(String::new()),
            ..WebMirrorConfig::default()
        };
        assert!(
            !empty.has_credential(),
            "an empty password is not a credential"
        );
        let with_pw = WebMirrorConfig {
            password: Some("x".into()),
            ..WebMirrorConfig::default()
        };
        assert!(with_pw.has_credential());
        let with_hash = WebMirrorConfig {
            hashed_password: Some("$argon2id$...".into()),
            ..WebMirrorConfig::default()
        };
        assert!(with_hash.has_credential());
    }

    #[test]
    fn web_validation_rejects_port_zero_when_enabled() {
        let mut cfg = Config::default();
        cfg.web_mirror.enabled = true;
        cfg.web_mirror.port = 0;
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn web_validation_rejects_bad_bind_when_enabled() {
        let mut cfg = Config::default();
        cfg.web_mirror.enabled = true;
        cfg.web_mirror.bind = "not-an-ip".into();
        assert!(validate_config(&cfg).is_err());
    }

    #[test]
    fn web_validation_ignores_bind_and_port_when_disabled() {
        // A disabled web section is never acted on, so its fields are not
        // range-checked — a stale/garbage value must not block startup.
        let mut cfg = Config::default();
        cfg.web_mirror.enabled = false;
        cfg.web_mirror.port = 0;
        cfg.web_mirror.bind = "not-an-ip".into();
        assert!(validate_config(&cfg).is_ok());
    }

    #[test]
    fn generate_password_has_expected_length_and_alphabet() {
        let pw = generate_password().unwrap();
        assert_eq!(pw.chars().count(), GENERATED_PASSWORD_LEN);
        assert!(
            pw.bytes().all(|b| PASSWORD_ALPHABET.contains(&b)),
            "generated password must only use the unambiguous TOML-safe alphabet"
        );
        // Two draws should differ with overwhelming probability.
        assert_ne!(pw, generate_password().unwrap());
    }

    #[test]
    fn insert_password_adds_line_under_existing_header() {
        let source = "[web_mirror]\nenabled = true\nport = 8090\n";
        let out = insert_password(source, WEB_MIRROR_TABLE, "secret");
        assert_eq!(
            out, "[web_mirror]\npassword = \"secret\"\nenabled = true\nport = 8090\n",
            "the password line must land right after the [web_mirror] header"
        );
        // The result round-trips and exposes the password.
        let cfg: Config = toml::from_str(&out).unwrap();
        assert_eq!(cfg.web_mirror.password.as_deref(), Some("secret"));
    }

    #[test]
    fn insert_password_appends_table_when_absent() {
        let source = "[layout]\nupper_pct = 55\n";
        let out = insert_password(source, WEB_MIRROR_TABLE, "secret");
        assert!(out.starts_with(source));
        assert!(out.contains("\n[web_mirror]\npassword = \"secret\"\n"));
        let cfg: Config = toml::from_str(&out).unwrap();
        assert_eq!(cfg.web_mirror.password.as_deref(), Some("secret"));
    }

    #[test]
    fn insert_password_appends_table_into_empty_source() {
        let out = insert_password("", WEB_MIRROR_TABLE, "secret");
        assert_eq!(out, "[web_mirror]\npassword = \"secret\"\n");
        let cfg: Config = toml::from_str(&out).unwrap();
        assert_eq!(cfg.web_mirror.password.as_deref(), Some("secret"));
    }

    #[test]
    fn the_web_mirror_table_configures_the_mirror() {
        let cfg: Config = toml::from_str("[web_mirror]\nenabled = true\nport = 8100\n").unwrap();

        assert!(cfg.web_mirror.enabled);
        assert_eq!(cfg.web_mirror.port, 8100);
    }

    #[test]
    fn insert_password_targets_the_named_table() {
        // The viewer's credential must land under [web_viewer], not [web] —
        // writing it to the wrong table would silently give the mirror a
        // second password and leave the viewer without one.
        let source = "[web_mirror]\nport = 8090\n";

        let out = insert_password(source, WEB_VIEWER_TABLE, "vsecret");

        assert!(
            out.contains("[web_viewer]\npassword = \"vsecret\""),
            "got: {out}"
        );
        let web_table = out.split("[web_viewer]").next().unwrap();
        assert!(
            !web_table.contains("vsecret"),
            "the viewer password leaked into [web_mirror]: {out}"
        );
    }

    #[test]
    fn insert_password_finds_an_existing_viewer_table() {
        let source = "[web_mirror]\nport = 8090\n\n[web_viewer]\nport = 8091\n";

        let out = insert_password(source, WEB_VIEWER_TABLE, "vsecret");

        let viewer = out.split("[web_viewer]").nth(1).unwrap();
        assert!(viewer.contains("password = \"vsecret\""), "got: {out}");
        assert_eq!(out.matches("[web_viewer]").count(), 1, "no duplicate table");
    }

    #[test]
    fn insert_password_ignores_commented_header() {
        // A `# [web]` comment is not a real table header, so the password must
        // be appended as a new table rather than inserted under the comment.
        let source = "# [web] example\nfoo = 1\n";
        let out = insert_password(source, WEB_MIRROR_TABLE, "secret");
        assert!(out.contains("\n[web_mirror]\npassword = \"secret\"\n"));
    }

    #[test]
    fn ensure_web_password_is_noop_when_credential_present() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.web_mirror.enabled = true;
        cfg.web_mirror.password = Some("preset".into());
        let generated = ensure_web_mirror_password(&mut cfg, &path).unwrap();
        assert!(
            generated.is_none(),
            "an existing credential must not be replaced"
        );
        assert!(
            !path.exists(),
            "no file should be written when a password exists"
        );
        assert_eq!(cfg.web_mirror.password.as_deref(), Some("preset"));
    }

    #[test]
    fn ensure_web_password_generates_persists_and_sets() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "[web_mirror]\nenabled = true\n").unwrap();

        let mut cfg = Config::default();
        cfg.web_mirror.enabled = true;
        let generated = ensure_web_mirror_password(&mut cfg, &path).unwrap();

        let pw = generated.expect("a password must be generated when none is set");
        assert_eq!(cfg.web_mirror.password.as_deref(), Some(pw.as_str()));
        // The persisted file now parses back to the same password.
        let reparsed: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(reparsed.web_mirror.password.as_deref(), Some(pw.as_str()));
    }

    #[test]
    fn ensure_web_password_creates_file_when_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml");
        let mut cfg = Config::default();
        cfg.web_mirror.enabled = true;

        let pw = ensure_web_mirror_password(&mut cfg, &path)
            .unwrap()
            .unwrap();

        assert!(path.exists());
        let reparsed: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(reparsed.web_mirror.password.as_deref(), Some(pw.as_str()));
    }

    #[test]
    fn log_config_parses_from_toml() {
        let toml = r#"
[log]
enabled = false
prompt_log = true
rotation = "size"
max_size_mb = 5
max_days = 14
level = "debug"
dir = "/tmp/logs"
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(!cfg.log.enabled);
        assert!(cfg.log.prompt_log);
        assert_eq!(cfg.log.rotation, LogRotation::Size);
        assert_eq!(cfg.log.max_size_mb, 5);
        assert_eq!(cfg.log.max_days, 14);
        assert_eq!(cfg.log.level, LogLevel::Debug);
        assert_eq!(cfg.log.dir, "/tmp/logs");
    }
}
