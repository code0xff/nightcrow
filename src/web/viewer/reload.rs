//! Re-reading `config.toml` into a running session, independent of how the
//! asking arrived.
//!
//! Sits beside [`session`](super::session) and for the same reason: the browser
//! reaches this over HTTP and an attached terminal over the daemon socket, and
//! both must land on exactly the same state change. Neither transport
//! authenticates here — deciding who may ask is theirs, and they answer it
//! differently (a session cookie on one side, being the user who owns a 0600
//! socket on the other).
//!
//! **What a reload is, and what it is not.** It re-reads two tables and nothing
//! else. `[[plugin]]` reaches even the repositories that are already open,
//! because a plugin is a child process and replacing one costs the session
//! nothing. `[[startup_command]]` reaches only the repositories opened
//! afterwards: a hub creates its startup panes once for its life, and the panes a
//! running repository already spent that list on are live children — an agent
//! CLI mid-task — that no file edit may replace. Everything else in the file
//! (the listener's address and password, logging, the client-owned layout and
//! input sections) is read once at startup and still needs a restart.
//!
//! **It does not half-apply.** The whole file is parsed and validated first, so a
//! typo anywhere leaves the session exactly as it was and the error goes back to
//! whoever asked.

use super::server::ViewerState;

/// Why a reload could not be carried out. The session is untouched in every case.
#[derive(Debug)]
pub enum ReloadError {
    /// The file could not be read, did not parse, or failed validation. Carries
    /// the message as written for the operator — these name the offending key,
    /// which is the whole value of reporting it.
    Config(anyhow::Error),
}

impl std::fmt::Display for ReloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // `{:#}` so the anyhow context chain reads as one line: the outer
            // frame says which file, the inner one says which key.
            Self::Config(err) => write!(f, "{err:#}"),
        }
    }
}

/// What a reload applied, for the client that asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadReport {
    /// `[[plugin]]` entries now declared.
    pub plugins: usize,
    /// Configured startup panes now on file, before `--exec` is merged in.
    pub startup_commands: usize,
    /// Repositories whose plugins were re-applied.
    pub repos: usize,
}

impl ReloadReport {
    /// One line for a person: what landed, and what is waiting on a repository
    /// being opened.
    ///
    /// Written here rather than in each client because both surfaces show the
    /// same sentence — a toast in the browser, a notice in the TUI — and two
    /// wordings of the same outcome would drift.
    pub fn summary(&self) -> String {
        let panes = if self.startup_commands == 0 {
            "no startup panes configured".to_string()
        } else {
            format!(
                "{} startup pane{} apply to newly opened projects",
                self.startup_commands,
                if self.startup_commands == 1 { "" } else { "s" }
            )
        };
        format!(
            "config reloaded: {} plugin{} across {} open project{}; {panes}",
            self.plugins,
            if self.plugins == 1 { "" } else { "s" },
            self.repos,
            if self.repos == 1 { "" } else { "s" },
        )
    }
}

/// Re-read the config file and apply the two tables it owns.
///
/// Serialized against itself by [`ViewerState::reload_lock`]: two clients
/// pressing at once would otherwise interleave a table swap with another's
/// fan-out, and the hubs could end up told about different files. The second
/// caller waits and then re-reads, so it applies the same file rather than a
/// stale copy of it.
pub fn reload_config(state: &ViewerState) -> Result<ReloadReport, ReloadError> {
    let path = crate::config::config_file_path().map_err(ReloadError::Config)?;
    reload_config_at(state, &path)
}

/// Path-explicit core of [`reload_config`], so the behaviour is testable against
/// a temp file rather than the caller's real `~/.nightcrow/config.toml`.
pub fn reload_config_at(
    state: &ViewerState,
    path: &std::path::Path,
) -> Result<ReloadReport, ReloadError> {
    let _serialized = state
        .reload_lock
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Read and validated before anything is touched, so a bad file is a refusal
    // rather than a partly-reconfigured session. A file that has gone missing is
    // reported too: at reload time that is a mistake, not the "nothing configured
    // yet" state a fresh install starts in.
    let cfg = crate::config::read_config_file(path).map_err(ReloadError::Config)?;

    state
        .catalog
        .set_config_tables(&cfg.startup_commands, cfg.plugins.clone())
        .map_err(ReloadError::Config)?;

    // Then the repositories already open. Each hub is *asked* — the work happens
    // on its own worker thread, which is the only thread allowed to touch a
    // plugin child — so this returns before the children have finished being
    // replaced. That is deliberate: waiting would mean blocking whoever asked on
    // every repository's queue.
    let entries = state.catalog.entries();
    for entry in &entries {
        entry.terminals.reload_plugins(cfg.plugins.clone());
    }

    tracing::info!(
        plugins = cfg.plugins.len(),
        startup_commands = cfg.startup_commands.len(),
        repos = entries.len(),
        "session: re-read the config file"
    );
    Ok(ReloadReport {
        plugins: cfg.plugins.len(),
        startup_commands: cfg.startup_commands.len(),
        repos: entries.len(),
    })
}

#[cfg(test)]
#[path = "reload_tests.rs"]
mod tests;
