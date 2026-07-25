use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod layout;
mod log;
mod panels;
mod web;

pub use layout::{Accent, InputConfig, LayoutConfig, StartupCommand, ThemeConfig, parse_leader};
#[cfg(test)]
pub use log::LogLevel;
pub use log::{LogConfig, LogRotation};
pub use panels::{AgentIndicatorConfig, MouseConfig, TreeConfig};
#[cfg(test)]
pub use web::generate_password;
pub use web::{
    WebMirrorConfig, WebViewerConfig, ensure_web_mirror_password, ensure_web_viewer_password,
};

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
pub(super) fn write_config_template(path: &std::path::Path, force: bool) -> Result<InitOutcome> {
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

pub fn validate_config(cfg: &Config) -> Result<()> {
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
mod tests;
