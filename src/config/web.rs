use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Default TCP port for the web viewer server.
const DEFAULT_WEB_VIEWER_PORT: u16 = 8091;
pub(super) const WEB_VIEWER_TABLE: &str = "web_viewer";
/// Default bind address: loopback only. Exposing the server on a routable
/// address is a deliberate opt-in because it grants live control of a shell.
const DEFAULT_WEB_BIND: &str = "127.0.0.1";
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
}

impl Default for WebViewerConfig {
    fn default() -> Self {
        Self {
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
pub(super) fn insert_password(source: &str, table: &str, password: &str) -> String {
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

fn table_header_line_end(source: &str, table: &str) -> Option<usize> {
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if line.trim() == format!("[{table}]") {
            return Some(offset + line.trim_end_matches('\n').len());
        }
        offset += line.len();
    }
    None
}

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
