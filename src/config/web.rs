use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::Write;
use toml_edit::{DocumentMut, Item, Table, Value};

/// Default TCP port for the web viewer server.
const DEFAULT_WEB_VIEWER_PORT: u16 = 8091;
const WEB_VIEWER_TABLE: &str = "web_viewer";
/// Default bind address: loopback only. Exposing the server on a routable
/// address is a deliberate opt-in because it grants live control of a shell.
const DEFAULT_WEB_BIND: &str = "127.0.0.1";
/// How long a browser login lasts by default.
const DEFAULT_SESSION_TTL_HOURS: u64 = 24;
/// `session_ttl_hours = 0` means no expiry, the same sentinel `log.max_days`
/// uses for "keep forever".
const NO_SESSION_EXPIRY: u64 = 0;
/// Length (characters) of an auto-generated web password.
pub(super) const GENERATED_PASSWORD_LEN: usize = 24;
/// Alphabet for generated passwords: alphanumeric minus visually ambiguous
/// glyphs (0/O, 1/l/I). All chars are TOML-safe, so the persisted value never
/// needs escaping when written as a basic `"..."` string.
pub(super) const PASSWORD_ALPHABET: &[u8] =
    b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// The native web viewer (`[web_viewer]`): its own port, cookie, and
/// credential, kept separate from anything else the process may serve.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WebViewerConfig {
    /// Address to bind. Loopback by default; the server speaks plain HTTP, so
    /// remote access belongs behind an SSH tunnel or reverse proxy.
    pub bind: String,
    pub port: u16,
    /// Plaintext login password, generated and written back on first enable.
    pub password: Option<String>,
    /// Optional Argon2 PHC hash. Takes precedence over `password`.
    pub hashed_password: Option<String>,
    /// How long a browser login lasts before it has to be repeated. `0` means
    /// it never expires on its own — whoever runs the daemon decides how much
    /// of a re-login this surface is worth, and on a loopback-bound session
    /// there may be no one to re-authenticate against.
    pub session_ttl_hours: u64,
}

impl Default for WebViewerConfig {
    fn default() -> Self {
        Self {
            bind: DEFAULT_WEB_BIND.to_string(),
            port: DEFAULT_WEB_VIEWER_PORT,
            password: None,
            hashed_password: None,
            session_ttl_hours: DEFAULT_SESSION_TTL_HOURS,
        }
    }
}

impl WebViewerConfig {
    pub fn has_credential(&self) -> bool {
        self.hashed_password.is_some() || self.password.as_deref().is_some_and(|p| !p.is_empty())
    }

    /// The configured login lifetime, or `None` when it never expires.
    pub fn session_ttl(&self) -> Option<std::time::Duration> {
        (self.session_ttl_hours != NO_SESSION_EXPIRY)
            .then(|| std::time::Duration::from_secs(self.session_ttl_hours * 3600))
    }
}

/// Generate a random, human-readable password from the OS RNG. The modulo
/// reduction introduces a negligible bias (256 mod 55) that is immaterial for
/// a locally-scoped dev credential; `getrandom` is the same OS entropy source
/// Argon2 salts use.
pub fn generate_password() -> Result<String> {
    let mut bytes = [0u8; GENERATED_PASSWORD_LEN];
    getrandom::fill(&mut bytes)
        .map_err(|e| anyhow::anyhow!("OS RNG unavailable for web password generation: {e}"))?;
    Ok(bytes
        .iter()
        .map(|b| PASSWORD_ALPHABET[usize::from(*b) % PASSWORD_ALPHABET.len()] as char)
        .collect())
}

/// Ensure the viewer has a login credential, generating and persisting
/// one when the config has none. A no-op when a `password` or `hashed_password`
/// is already set. Otherwise a random password is generated, written back into
/// the config file at `path` (creating it if absent, preserving any existing
/// content and comments), and stored on `cfg` so the running instance uses it.
/// Returns the freshly generated password so the caller can surface it to the
/// user, or `None` when a credential already existed.
pub fn ensure_web_viewer_password(
    cfg: &mut super::Config,
    path: &std::path::Path,
) -> Result<Option<String>> {
    if cfg.web_viewer.has_credential() {
        return Ok(None);
    }
    let password = generate_password()?;
    persist_password(path, &password).with_context(|| {
        format!(
            "persisting generated web viewer password to {}",
            path.display()
        )
    })?;
    cfg.web_viewer.password = Some(password.clone());
    Ok(Some(password))
}

/// Write `password` into the `[web_viewer]` table of the TOML file at `path`.
fn persist_password(path: &std::path::Path, password: &str) -> Result<()> {
    let existing = if path.exists() {
        std::fs::read_to_string(path)
            .with_context(|| format!("reading config file {}", path.display()))?
    } else {
        String::new()
    };
    let updated = upsert_password(&existing, password)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(parent)
        .with_context(|| format!("creating config directory {}", parent.display()))?;

    let mut pending = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating a temporary file in {}", parent.display()))?;
    pending
        .write_all(updated.as_bytes())
        .with_context(|| format!("writing config file {}", path.display()))?;
    pending
        .as_file_mut()
        .flush()
        .with_context(|| format!("flushing config file {}", path.display()))?;
    restrict_permissions(pending.path())?;
    pending
        .persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("replacing config file {}", path.display()))?;
    Ok(())
}

/// Format-preserving TOML transform behind [`persist_password`].
pub(super) fn upsert_password(source: &str, password: &str) -> Result<String> {
    let uses_crlf = source.contains("\r\n");
    let mut document = source
        .parse::<DocumentMut>()
        .context("parsing config before persisting the web viewer password")?;

    if document.get(WEB_VIEWER_TABLE).is_none() {
        document.insert(WEB_VIEWER_TABLE, Item::Table(Table::new()));
    }

    let table_item = document
        .get_mut(WEB_VIEWER_TABLE)
        .expect("the web viewer table was inserted above");
    let table = table_item
        .as_table_like_mut()
        .context("web_viewer must be a TOML table")?;

    if let Some(existing) = table.get_mut("password") {
        let decor = existing
            .as_value()
            .context("web_viewer.password must be a TOML value")?
            .decor()
            .clone();
        let mut replacement = Value::from(password);
        *replacement.decor_mut() = decor;
        *existing = Item::Value(replacement);
    } else {
        table.insert("password", Item::Value(Value::from(password)));
    }

    let output = document.to_string();
    Ok(if uses_crlf {
        output.replace('\n', "\r\n")
    } else {
        output
    })
}

fn restrict_permissions(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting config permissions on {}", path.display()))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
