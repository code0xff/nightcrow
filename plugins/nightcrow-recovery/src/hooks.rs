//! Install and uninstall this plugin's entries in Claude Code's
//! `~/.claude/settings.json`.
//!
//! That file belongs to the user, not to us: it may hold keys and hook events we
//! know nothing about, so every edit here is a merge that preserves what we did
//! not put there, and a refusal when the file cannot be understood. The JSON
//! surgery itself lives in [`merge`]; this module is the filesystem around it.

use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "hooks_merge.rs"]
mod merge;

use merge::{MARKER, STATUSLINE_KEY};

/// Re-exported for the statusline helper, which must recognise our own command in
/// order to refuse chaining to it — the same check install and uninstall use to
/// recognise our entries.
pub(crate) use merge::is_ours;

const CLAUDE_DIR: &str = ".claude";
const SETTINGS_FILE: &str = "settings.json";
const BACKUP_FILE: &str = "settings.json.bak";
/// Sidecar name: prefixed with the crate name because it lives in a directory
/// the provider owns, and must never look like something the provider wrote.
const SIDECAR_FILE: &str = "nightcrow-recovery.displaced.json";

/// `settings.json` is user configuration in the user's home directory: readable
/// and writable by its owner only. On Windows the default ACL on a user-home
/// file already suffices, so the mode is only applied on Unix.
const SETTINGS_MODE: u32 = 0o600;

/// Restrict a path to its owner on platforms where that is meaningful.
#[cfg(unix)]
fn restrict_to_owner(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|e| anyhow::anyhow!("cannot set mode on {}: {e}", path.display()))
}

#[cfg(not(unix))]
fn restrict_to_owner(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

/// Where the Claude Code settings we edit live. A struct so tests can point at a
/// temp dir instead of the real home directory.
#[derive(Debug, Clone)]
pub struct SettingsPaths {
    pub settings: PathBuf,
    pub backup: PathBuf,
    /// Sidecar holding what we displaced, so uninstall can put it back.
    pub sidecar: PathBuf,
}

impl SettingsPaths {
    /// `~/.claude/settings.json` and its siblings.
    pub fn from_home(home: &Path) -> Self {
        let dir = home.join(CLAUDE_DIR);
        Self {
            settings: dir.join(SETTINGS_FILE),
            backup: dir.join(BACKUP_FILE),
            sidecar: dir.join(SIDECAR_FILE),
        }
    }

    /// Resolves the home directory, erroring when it cannot be determined.
    pub fn discover() -> Result<Self> {
        let home =
            dirs::home_dir().context("no home directory, so ~/{CLAUDE_DIR} cannot be located")?;
        Ok(Self::from_home(&home))
    }
}

/// Install our `StopFailure` hook and statusline entries, merging into whatever
/// is already there. Returns one line per change, for printing.
pub fn install(paths: &SettingsPaths, exe: &str) -> Result<Vec<String>> {
    let command = merge::hook_command(exe);
    anyhow::ensure!(
        merge::is_ours(&command),
        "hook command `{command}` does not contain `{MARKER}`, so uninstall could not \
         recognise it later; install through a binary whose path contains `{MARKER}`"
    );

    let mut settings = read_settings(&paths.settings)?;
    let (mut changes, displaced) = merge::merge_into(&mut settings, exe)
        .with_context(|| format!("cannot merge our entries into {}", paths.settings.display()))?;
    if changes.is_empty() {
        return Ok(vec![format!(
            "{} already has our hook and statusline; nothing changed",
            paths.settings.display()
        )]);
    }

    back_up(&paths.settings, &paths.backup)?;
    if let Some(previous) = displaced {
        let mut sidecar = Map::new();
        sidecar.insert(STATUSLINE_KEY.to_string(), previous);
        write_json(&paths.sidecar, &Value::Object(sidecar))?;
        changes.push(format!(
            "recorded the displaced {STATUSLINE_KEY} in {}",
            paths.sidecar.display()
        ));
    }
    write_json(&paths.settings, &settings)?;
    Ok(changes)
}

/// Remove only what [`install`] added. Returns one line per change.
pub fn uninstall(paths: &SettingsPaths) -> Result<Vec<String>> {
    if !paths.settings.exists() {
        return Ok(vec![format!(
            "{} does not exist; nothing to remove",
            paths.settings.display()
        )]);
    }
    let mut settings = read_settings(&paths.settings)?;
    let restore = read_sidecar(&paths.sidecar);
    let changes = merge::strip_from(&mut settings, restore).with_context(|| {
        format!(
            "cannot remove our entries from {}",
            paths.settings.display()
        )
    })?;
    if changes.is_empty() {
        return Ok(vec![format!(
            "{} has none of our entries; nothing to remove",
            paths.settings.display()
        )]);
    }

    write_json(&paths.settings, &settings)?;
    if paths.sidecar.exists() {
        fs::remove_file(&paths.sidecar)
            .with_context(|| format!("cannot delete {}", paths.sidecar.display()))?;
    }
    Ok(changes)
}

/// The `statusLine` install displaced, for the statusline helper to chain to.
///
/// Read on every refresh, which is why it stays this cheap: one small file, and
/// any trouble reading it means no chain rather than a failure. The value can be
/// JSON `null` — that is what install records when it found no `statusLine` to
/// displace — so a caller must decide what a null means to it.
pub fn displaced_statusline(paths: &SettingsPaths) -> Option<Value> {
    read_sidecar(&paths.sidecar)
}

/// Absent or empty means "no settings yet"; anything that is not a JSON object
/// is a file we do not understand, and guessing at it is worse than stopping.
fn read_settings(path: &Path) -> Result<Value> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(anyhow::anyhow!("cannot read {}: {e}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!(
            "{} is not valid JSON ({e}); refusing to edit a file we cannot parse",
            path.display()
        )
    })?;
    anyhow::ensure!(
        value.is_object(),
        "{} holds a JSON {} at its top level, not an object; refusing to edit it",
        path.display(),
        merge::kind_of(&value)
    );
    Ok(value)
}

/// The recorded `statusLine`, or `None` when there is no usable sidecar. A
/// damaged sidecar is not fatal: the worst outcome is that we drop a key the user
/// can set again, whereas failing would leave our entries installed for good.
fn read_sidecar(path: &Path) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value.get(STATUSLINE_KEY).cloned()
}

/// Copy the current settings verbatim before the first write, so a bad merge is
/// recoverable. Nothing to copy when the file does not exist yet, and in that
/// case an older backup from a previous install is left as it is.
fn back_up(settings: &Path, backup: &Path) -> Result<()> {
    if !settings.exists() {
        return Ok(());
    }
    fs::copy(settings, backup).map_err(|e| {
        anyhow::anyhow!(
            "cannot copy {} to {}: {e}",
            settings.display(),
            backup.display()
        )
    })?;
    Ok(())
}

/// Write via a temp file in the same directory and rename over the target, so a
/// crash mid-write cannot leave the provider with a half-written settings file.
fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .map_err(|e| anyhow::anyhow!("cannot create {}: {e}", dir.display()))?;
    }
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|e| anyhow::anyhow!("cannot encode JSON for {}: {e}", path.display()))?;
    text.push('\n');

    let temp = path.with_extension("json.tmp");
    fs::write(&temp, &text).map_err(|e| anyhow::anyhow!("cannot write {}: {e}", temp.display()))?;
    // Mode is set before the rename, so the target is never briefly world-readable.
    // On Windows the default ACL on a user-home file already suffices.
    restrict_to_owner(&temp, SETTINGS_MODE)?;
    fs::rename(&temp, path).map_err(|e| {
        anyhow::anyhow!(
            "cannot rename {} to {}: {e}",
            temp.display(),
            path.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
#[path = "hooks_tests.rs"]
mod tests;
