use serde::{Deserialize, Serialize};

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
