use crate::config::{LogConfig, LogRotation};
use std::fs;
use std::path::{Path, PathBuf};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

mod rotation;
use rotation::{SizeRollingAppender, TimeRotationCleanup, cleanup_old_logs};

/// The directory nightcrow keeps its own files in, inside a repository.
const NIGHTCROW_DIR: &str = ".nightcrow";

const LOG_FILE_PREFIX: &str = "nightcrow.log";
const LOG_FILE_PREFIX_WITH_SEPARATOR: &str = "nightcrow.log.";
const BYTES_PER_MB: u64 = 1 << 20;

pub struct LogGuard {
    _guard: WorkerGuard,
}

pub fn init_logging(config: &LogConfig, repo_path: &str) -> Option<LogGuard> {
    if !config.enabled {
        return None;
    }

    let log_dir = resolve_log_dir(&config.dir, repo_path);
    if let Err(e) = fs::create_dir_all(&log_dir) {
        // Subscriber install hasn't happened yet, so tracing would go to a
        // no-op writer. Print to stderr so the user can see why "log.enabled
        // = true" produced no file.
        eprintln!(
            "nightcrow: failed to create log directory {}: {e}",
            log_dir.display()
        );
        return None;
    }
    write_log_gitignore(&log_dir);
    cleanup_old_logs(&log_dir, config.max_days);

    let level = config.level.as_str();
    // `prompt` is a dedicated tracing target for terminal prompt capture. We
    // pin it at info regardless of the global level so that enabling
    // `prompt_log` always produces output.
    let filter_str = if config.prompt_log {
        format!("{level},prompt=info")
    } else {
        level.to_string()
    };

    let (writer, guard) = match config.rotation {
        LogRotation::Hourly => {
            let appender = tracing_appender::rolling::hourly(&log_dir, LOG_FILE_PREFIX);
            let appender = TimeRotationCleanup::new(appender, &log_dir, config.max_days, 3_600);
            tracing_appender::non_blocking(appender)
        }
        LogRotation::Size => {
            let max_bytes = config.max_size_mb.saturating_mul(BYTES_PER_MB);
            if let Some(appender) =
                SizeRollingAppender::new(&log_dir, LOG_FILE_PREFIX, max_bytes, config.max_days)
            {
                tracing_appender::non_blocking(appender)
            } else {
                eprintln!(
                    "nightcrow: failed to open size-based log appender in {}; falling back to daily rotation",
                    log_dir.display()
                );
                let appender = tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
                let appender =
                    TimeRotationCleanup::new(appender, &log_dir, config.max_days, 86_400);
                tracing_appender::non_blocking(appender)
            }
        }
        LogRotation::Daily => {
            let appender = tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
            let appender = TimeRotationCleanup::new(appender, &log_dir, config.max_days, 86_400);
            tracing_appender::non_blocking(appender)
        }
    };

    let filter = EnvFilter::try_new(&filter_str).unwrap_or_else(|_| EnvFilter::new("warn"));

    let file_layer = fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true);

    let subscriber = tracing_subscriber::registry().with(filter).with(file_layer);

    if let Err(e) = tracing::subscriber::set_global_default(subscriber) {
        // A prior `set_global_default` in this process (rare but possible
        // in tests / repeated init) silently drops every later log line —
        // surface it so the user can see why logs are empty.
        eprintln!("nightcrow: failed to install global tracing subscriber: {e}");
        return None;
    }

    Some(LogGuard { _guard: guard })
}

/// Drop a self-ignoring `.gitignore` in the log directory so logs never
/// pollute the user's `git status` — the default `.nightcrow/logs` sits
/// inside the repo.
///
/// Only into a directory nightcrow owns (one under `.nightcrow`): the `*`
/// pattern ignores the directory's every untracked file, which would be
/// wrong for a user-chosen `[log] dir` — that one is the user's to manage.
/// Only written when missing, so a user-edited file is not clobbered.
fn write_log_gitignore(log_dir: &Path) {
    if !log_dir.components().any(|c| c.as_os_str() == NIGHTCROW_DIR) {
        return;
    }
    let gitignore = log_dir.join(".gitignore");
    if !gitignore.exists()
        && let Err(e) = fs::write(&gitignore, "*\n")
    {
        eprintln!("nightcrow: failed to write log gitignore: {e}");
    }
}

fn resolve_log_dir(dir: &str, repo_path: &str) -> PathBuf {
    let path = PathBuf::from(dir);
    if path.is_absolute() {
        path
    } else {
        PathBuf::from(repo_path).join(path)
    }
}

#[cfg(test)]
#[path = "logging_tests.rs"]
mod tests;
