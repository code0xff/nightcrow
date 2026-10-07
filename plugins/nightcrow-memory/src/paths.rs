//! Where one repository's shared notes live, and who is asking.
//!
//! Both answers come from the environment nightcrow gives every pane, so two
//! agents in panes of the same project reach the same file without being
//! configured to, and an agent started anywhere else reaches nothing.

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Set by nightcrow on every pane: the directory its project's plugins use.
/// Only its last component is read here — the per-project key.
pub const RUNTIME_DIR_ENV: &str = "NIGHTCROW_PLUGIN_RUNTIME_DIR";
/// Set by nightcrow on every pane: a random name for that pane.
pub const PANE_TOKEN_ENV: &str = "NIGHTCROW_PANE_TOKEN";
/// Overrides the directory the databases are kept in.
pub const STORE_DIR_ENV: &str = "NIGHTCROW_MEMORY_DIR";

/// How much of the pane token an author label shows. Enough to tell the panes
/// of one project apart; not the token, which is the name a plugin would use to
/// ask the host for that pane.
const TOKEN_LABEL_CHARS: usize = 6;
const PROJECT_KEY_LEN: usize = 16;

/// The project key nightcrow derived for this pane's repository.
///
/// It becomes a file name, so it is checked rather than trusted: exactly the
/// fixed-width lowercase hex nightcrow writes, which cannot name a path.
pub fn project_key(runtime_dir: Option<&str>) -> Result<String> {
    let Some(dir) = runtime_dir.filter(|d| !d.is_empty()) else {
        bail!(
            "{RUNTIME_DIR_ENV} is not set: shared memory belongs to a nightcrow project, \
             and this process was not started inside one of its panes"
        );
    };
    let key = Path::new(dir)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let well_formed = key.len() == PROJECT_KEY_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if !well_formed {
        bail!("{RUNTIME_DIR_ENV} does not end in a nightcrow project key: {dir:?}");
    }
    Ok(key.to_string())
}

/// The directory the databases are kept in: the override, else
/// `~/.nightcrow/memory`. Never inside the repository — a working tree is the
/// user's, and a file there would show up in their status and their commits.
pub fn store_dir(override_dir: Option<&str>, home: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = override_dir.filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    let home = home.context("no home directory to keep shared memory under")?;
    Ok(home.join(".nightcrow").join("memory"))
}

pub fn db_path(store_dir: &Path, project_key: &str) -> PathBuf {
    store_dir.join(format!("{project_key}.db"))
}

/// Who a note is from, as other agents will read it: the program that
/// connected, and which pane it is in.
///
/// A label, not an identity. Nothing checks it, and nothing should rely on it
/// for more than telling two writers apart.
pub fn author_label(client: Option<&str>, pane_token: Option<&str>) -> String {
    let client: String = client
        .unwrap_or("agent")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(32)
        .collect();
    let client = if client.is_empty() {
        "agent".to_string()
    } else {
        client
    };
    let pane: String = pane_token
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(TOKEN_LABEL_CHARS)
        .collect();
    if pane.is_empty() {
        client
    } else {
        format!("{client}@{pane}")
    }
}

/// The store for the pane this process runs in, read from the real environment.
pub fn db_path_from_env() -> Result<PathBuf> {
    let key = project_key(std::env::var(RUNTIME_DIR_ENV).ok().as_deref())?;
    let dir = store_dir(
        std::env::var(STORE_DIR_ENV).ok().as_deref(),
        home_dir().as_deref(),
    )?;
    Ok(db_path(&dir, &key))
}

fn home_dir() -> Option<PathBuf> {
    // `USERPROFILE` is Windows' answer; `HOME` is everyone else's, and is also
    // set by the shells people run on Windows.
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|name| std::env::var_os(name).filter(|v| !v.is_empty()))
        .map(PathBuf::from)
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;
