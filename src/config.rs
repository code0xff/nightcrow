use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub layout: LayoutConfig,
    pub log: LogConfig,
    pub theme: ThemeConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    Yellow,
    #[default]
    Green,
    Cyan,
    Magenta,
    Blue,
}

impl Accent {
    // Order MUST match the historical ACCENT_PRESETS slice
    // ["yellow", "cyan", "green", "magenta", "blue"] so that accent_idx
    // values persisted in pre-existing session.json files keep mapping
    // to the same color after the strong-enum migration.
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
        Self::ALL
            .iter()
            .position(|&a| a == self)
            .expect("ALL must contain every variant")
    }

    pub fn from_index(idx: usize) -> Accent {
        Self::ALL[idx % Self::ALL.len()]
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

fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("nightcrow").join("config.toml"))
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        validate_config(&Config::default()).unwrap();
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
