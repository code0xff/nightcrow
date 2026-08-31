//! Re-reading `config.toml` into a running session, independent of how the
//! asking arrived.
//!
//! Sits beside [`session`](super::session) for the same reason: the browser and
//! an attached terminal must land on exactly the same state change. Neither
//! transport authenticates here.
//!
//! **What a reload is, and what it is not.** `[[plugin]]` reaches even the
//! repositories that are already open, because replacing a plugin child costs
//! the session nothing. `[[startup_command]]` and `[terminal] auto_open` reach
//! only repositories opened afterwards: a hub decides its startup panes once
//! for its life, and no file edit may replace a running repository's children.
//!
//! **It does not half-apply.** The whole file is parsed and validated first, so
//! a typo anywhere leaves the session exactly as it was.

use super::SessionState;

/// Why a reload could not be carried out. The session is untouched in every case.
#[derive(Debug)]
pub enum ReloadError {
    /// The file could not be read, did not parse, or failed validation.
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
    /// Startup panes for new projects, including preserved `--exec` commands.
    pub startup_commands: usize,
    /// Whether a project with no startup commands opens one shell.
    pub auto_open: bool,
    /// Repositories whose plugins were re-applied.
    pub repos: usize,
    /// Repositories that could not even be asked, because their terminal worker
    /// was too far behind to take the request.
    pub unreachable: usize,
}

impl ReloadReport {
    /// One line for a person: what landed, and what is waiting on a repository
    /// being opened.
    ///
    /// Written here rather than in each client because both surfaces show the
    /// same sentence, and two wordings of the same outcome would drift.
    pub fn summary(&self) -> String {
        let panes = if self.startup_commands == 0 && self.auto_open {
            "1 automatic shell applies to newly opened projects".to_string()
        } else if self.startup_commands == 0 {
            "no startup panes configured".to_string()
        } else if self.startup_commands == 1 {
            "1 startup pane applies to newly opened projects".to_string()
        } else {
            format!(
                "{} startup panes apply to newly opened projects",
                self.startup_commands
            )
        };
        // Named only when it happened: a parenthesis on every ordinary reload
        // would be noise about a case that almost never occurs.
        let busy = match self.unreachable {
            0 => String::new(),
            1 => " (1 was too busy to be told)".to_string(),
            n => format!(" ({n} were too busy to be told)"),
        };
        format!(
            "config reloaded: {} plugin{} across {} open project{}{busy}; {panes}",
            self.plugins,
            if self.plugins == 1 { "" } else { "s" },
            self.repos,
            if self.repos == 1 { "" } else { "s" },
        )
    }
}

/// Re-read the config file and apply the live-reloadable settings it owns.
///
/// Serialized against itself by [`SessionState::reload_lock`]: two clients
/// pressing at once would otherwise interleave a table swap with another's
/// fan-out, and the hubs could end up told about different files. The second
/// caller waits and then re-reads, so it applies the same file rather than a
/// stale copy of it.
pub fn reload_config(state: &SessionState) -> Result<ReloadReport, ReloadError> {
    let path = crate::config::config_file_path().map_err(ReloadError::Config)?;
    reload_config_at(state, &path)
}

/// Path-explicit core of [`reload_config`], so the behaviour is testable against
/// a temp file rather than the caller's real `~/.nightcrow/config.toml`.
pub fn reload_config_at(
    state: &SessionState,
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

    // The entries come back from the swap rather than being fetched after it: a
    // repository opened in the same beat must either be in this list or have been
    // spawned from the new tables, never neither.
    let entries = state
        .catalog
        .set_config_tables(
            &cfg.startup_commands,
            cfg.terminal.clone(),
            cfg.plugins.clone(),
        )
        .map_err(ReloadError::Config)?;
    let startup_commands = state.catalog.startup_command_count();

    // Then the repositories already open. Each hub is *asked* — the work
    // happens on its own worker thread, the only thread allowed to touch a
    // plugin child — so this returns before the children have been replaced.
    // Deliberate: waiting would block whoever asked on every repository's
    // queue.
    //
    // A hub too far behind to take the request is counted rather than retried:
    // a full queue means its worker is wedged or being hammered, and neither
    // blocking on it nor pretending it complied is honest. It keeps the
    // plugins it had, and the report says so.
    let mut unreachable = 0;
    for entry in &entries {
        if entry.terminals.reload_plugins(cfg.plugins.clone()) {
            continue;
        }
        unreachable += 1;
        // Which repository, logged here: the hub does not keep its own path,
        // and the summary is one sentence for a person, too short to carry a
        // list.
        tracing::warn!(
            repo = %entry.path,
            "session: a repository's queue was full; its plugins were not re-applied"
        );
    }
    let asked = entries.len() - unreachable;

    tracing::info!(
        plugins = cfg.plugins.len(),
        startup_commands,
        repos = asked,
        unreachable,
        "session: re-read the config file"
    );
    Ok(ReloadReport {
        plugins: cfg.plugins.len(),
        startup_commands,
        auto_open: cfg.terminal.auto_open,
        repos: asked,
        unreachable,
    })
}

#[cfg(test)]
#[path = "reload_tests.rs"]
mod tests;
