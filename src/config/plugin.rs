use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// An external plugin process (`[[plugin]]`). nightcrow itself knows nothing
/// about what a plugin does: it launches the executable and speaks its protocol.
/// A plugin only ever sees a pane that opted in — by name in a
/// `[[startup_command]]`, or, with [`watch_on_signal`](Self::watch_on_signal),
/// by something inside the pane quoting the pane's own token — so adding a
/// plugin here does not hand it the whole session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Name a startup pane refers to in its `plugin =` field.
    pub name: String,
    /// Executable to run. Resolved against PATH and the plugin dir by the host.
    pub command: String,
    /// Arguments passed verbatim.
    #[serde(default)]
    pub args: Vec<String>,
    /// Extra environment for the plugin process only (NOT for panes).
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Flags this plugin may append when relaunching a pane's command.
    ///
    /// Empty by default, which refuses every relaunch that passes a flag. The
    /// core has no idea what any CLI's flags mean, so the user decides: a flag
    /// not listed here cannot be smuggled into the pane's command line, which
    /// is what keeps a plugin from quietly weakening a CLI's permission
    /// posture.
    #[serde(default)]
    pub allowed_resume_flags: Vec<String>,
    /// Off unless explicitly turned on.
    #[serde(default)]
    pub enabled: bool,
    /// Whether this plugin may be given a pane no `[[startup_command]]` named
    /// it in, when a process *inside* that pane reports to it quoting the
    /// pane's own token.
    ///
    /// Off by default, so an existing config keeps the property that the
    /// opt-in list is the whole of what a plugin can see. Turning it on trades
    /// that for a narrower one: a pane becomes reachable once something
    /// running in it has spoken to the plugin, which a plain shell never does.
    /// The token makes the difference — random, per pane, only in that pane's
    /// child environment, so it cannot be guessed from outside and the plugin
    /// is never told which panes exist.
    #[serde(default)]
    pub watch_on_signal: bool,
}

/// Check the `[[plugin]]` list and every startup pane's opt-in against it.
///
/// An opt-in naming a plugin that does not exist is an error, because the only
/// way to reach that state is a typo and the pane would silently never be
/// watched. Naming a plugin that exists but is disabled is allowed: `enabled`
/// is the switch for turning a plugin off without unpicking every pane that
/// refers to it, and "off" already means nothing happens.
pub(super) fn validate_plugins(cfg: &super::Config) -> Result<()> {
    anyhow::ensure!(
        cfg.plugins.len() <= super::MAX_PLUGINS,
        "at most {} [[plugin]] entries are allowed, found {}",
        super::MAX_PLUGINS,
        cfg.plugins.len()
    );
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (i, p) in cfg.plugins.iter().enumerate() {
        anyhow::ensure!(
            !p.name.trim().is_empty(),
            "plugin[{i}].name must not be empty"
        );
        anyhow::ensure!(
            !p.command.trim().is_empty(),
            "plugin[{i}].command must not be empty"
        );
        anyhow::ensure!(
            seen.insert(p.name.as_str()),
            "duplicate [[plugin]] name \"{}\"; plugin names must be unique so \
             a startup_command's plugin = \"...\" is unambiguous",
            p.name
        );
    }
    for (i, sc) in cfg.startup_commands.iter().enumerate() {
        let Some(name) = sc.plugin.as_deref() else {
            continue;
        };
        anyhow::ensure!(
            cfg.plugins.iter().any(|p| p.name == name),
            "startup_command[{i}].plugin \"{name}\" does not name any [[plugin]]"
        );
    }
    Ok(())
}
