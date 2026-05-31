use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Upper bound on `[[startup_command]]` entries. A small fixed cap keeps the
/// tab bar legible and startup bounded. Direct pane-jump keys cover the first
/// seven panes (`F3`..`F9` and `<leader> 1`..`7`); panes beyond that are still
/// reachable via focus cycling (`Shift+←/→`).
pub const MAX_STARTUP_COMMANDS: usize = 9;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub layout: LayoutConfig,
    pub log: LogConfig,
    pub theme: ThemeConfig,
    pub agent_indicator: AgentIndicatorConfig,
    pub input: InputConfig,
    /// Commands launched in their own terminal pane at startup, in order.
    /// Maps from TOML `[[startup_command]]` array-of-tables. Empty by
    /// default, which preserves the single empty-shell startup behaviour.
    #[serde(rename = "startup_command")]
    pub startup_commands: Vec<StartupCommand>,
}

/// Default leader chord literal. `Ctrl+G` avoids tmux's own `Ctrl+B` prefix
/// (so nightcrow can run inside tmux), and is rarely used by shells/readline,
/// leaving the terminal-editing Ctrl keys (`Ctrl+W`, `Ctrl+L`, …) free to
/// reach the PTY.
const DEFAULT_LEADER: &str = "ctrl+g";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InputConfig {
    /// The leader (prefix) chord. Every nightcrow app command is reached by
    /// pressing this key, then a follow-up key (tmux-style). Accepts a single
    /// `ctrl+<ascii>` chord; the parser rejects anything that doubles as a
    /// no-prefix reserved key (F1..F9, Shift+arrows, Shift+PgUp/PgDn).
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

fn default_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".nightcrow").join("config.toml"))
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
        // into Config and pass the same validation as a real user file.
        let toml = include_str!("../config.example.toml");
        let cfg: Config = toml::from_str(toml).expect("config.example.toml should parse");
        validate_config(&cfg).expect("config.example.toml should validate");
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
        for i in 0..5 {
            cfg.startup_commands.push(StartupCommand {
                name: None,
                command: format!("echo {i}"),
            });
        }
        // 5 config + 5 CLI = 10 > MAX_STARTUP_COMMANDS (9).
        let cli: Vec<String> = (0..5).map(|i| format!("run {i}")).collect();
        assert!(resolve_startup_commands(&cfg, &cli).is_err());
        // 5 config + 4 CLI = 9 is exactly the cap.
        let cli: Vec<String> = (0..4).map(|i| format!("run {i}")).collect();
        assert_eq!(resolve_startup_commands(&cfg, &cli).unwrap().len(), 9);
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
    fn input_leader_defaults_to_ctrl_g() {
        let cfg = Config::default();
        assert_eq!(cfg.input.leader, "ctrl+g");
        let leader = parse_leader(&cfg.input.leader).unwrap();
        assert_eq!(leader.code, KeyCode::Char('g'));
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
