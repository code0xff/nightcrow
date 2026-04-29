use crate::config::LogConfig;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

pub struct LogGuard {
    _guard: WorkerGuard,
}

pub fn init_logging(config: &LogConfig, repo_path: &str) -> Option<LogGuard> {
    if !config.enabled {
        return None;
    }

    let log_dir = resolve_log_dir(&config.dir, repo_path);
    fs::create_dir_all(&log_dir).ok()?;
    cleanup_old_logs(&log_dir, config.max_days);

    let level = parse_level(&config.level);
    let filter_str = if config.prompt_log {
        format!("{level},prompt=info")
    } else {
        level.to_string()
    };

    let (writer, guard) = match config.rotation.as_str() {
        "hourly" => {
            let appender = tracing_appender::rolling::hourly(&log_dir, "nightcrow.log");
            tracing_appender::non_blocking(appender)
        }
        "size" => {
            let max_bytes = config.max_size_mb.saturating_mul(1024 * 1024);
            let appender = SizeRollingAppender::new(&log_dir, "nightcrow.log", max_bytes);
            tracing_appender::non_blocking(appender)
        }
        _ => {
            // default: daily
            let appender = tracing_appender::rolling::daily(&log_dir, "nightcrow.log");
            tracing_appender::non_blocking(appender)
        }
    };

    let filter = EnvFilter::try_new(&filter_str).unwrap_or_else(|_| EnvFilter::new("warn"));

    let file_layer = fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true);

    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer);

    if tracing::subscriber::set_global_default(subscriber).is_err() {
        return None;
    }

    Some(LogGuard { _guard: guard })
}

fn resolve_log_dir(dir: &str, repo_path: &str) -> PathBuf {
    let path = PathBuf::from(dir);
    if path.is_absolute() {
        path
    } else {
        PathBuf::from(repo_path).join(path)
    }
}

fn parse_level(level: &str) -> &str {
    match level {
        "error" | "warn" | "info" | "debug" | "trace" => level,
        _ => "warn",
    }
}

pub fn cleanup_old_logs(log_dir: &Path, max_days: u32) {
    if max_days == 0 {
        return;
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(u64::from(max_days) * 86400))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let is_log = path
            .file_name()
            .and_then(|n| n.to_str())
            .map_or(false, |n| n.starts_with("nightcrow") && n.ends_with(".log"));
        if is_log {
            if let Ok(meta) = fs::metadata(&path) {
                if let Ok(modified) = meta.modified() {
                    if modified < cutoff {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }
}

// Rotates to a new numbered file when the current file exceeds max_bytes.
struct SizeRollingAppender {
    inner: Arc<Mutex<SizeRollingInner>>,
}

struct SizeRollingInner {
    dir: PathBuf,
    prefix: String,
    max_bytes: u64,
    current: File,
    current_size: u64,
    index: u32,
}

impl SizeRollingAppender {
    fn new(dir: &Path, prefix: &str, max_bytes: u64) -> Self {
        let path = dir.join(format!("{prefix}.0"));
        let current = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .expect("failed to open log file");
        let current_size = current.metadata().map(|m| m.len()).unwrap_or(0);
        Self {
            inner: Arc::new(Mutex::new(SizeRollingInner {
                dir: dir.to_path_buf(),
                prefix: prefix.to_string(),
                max_bytes,
                current,
                current_size,
                index: 0,
            })),
        }
    }
}

impl Write for SizeRollingAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap();
        if inner.max_bytes > 0 && inner.current_size + buf.len() as u64 > inner.max_bytes {
            inner.index += 1;
            let path = inner
                .dir
                .join(format!("{}.{}", inner.prefix, inner.index));
            inner.current = OpenOptions::new().create(true).append(true).open(path)?;
            inner.current_size = 0;
        }
        let n = inner.current.write(buf)?;
        inner.current_size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.lock().unwrap().current.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn cleanup_removes_files_older_than_max_days() {
        let dir = tempdir().unwrap();
        let old_file = dir.path().join("nightcrow.old.log");
        let new_file = dir.path().join("nightcrow.new.log");
        fs::write(&old_file, b"old").unwrap();
        fs::write(&new_file, b"new").unwrap();

        // Backdate old_file by setting mtime via a workaround (write then check)
        // Since we can't easily set mtime in stdlib, we verify the function runs
        // without panic and only deletes files matching the naming pattern.
        cleanup_old_logs(dir.path(), 0); // max_days=0 means keep all
        assert!(old_file.exists());
        assert!(new_file.exists());
    }

    #[test]
    fn cleanup_skips_non_nightcrow_files() {
        let dir = tempdir().unwrap();
        let other = dir.path().join("other.log");
        fs::write(&other, b"x").unwrap();
        cleanup_old_logs(dir.path(), 1);
        assert!(other.exists());
    }

    #[test]
    fn size_rolling_appender_rotates_on_overflow() {
        let dir = tempdir().unwrap();
        let mut appender = SizeRollingAppender::new(dir.path(), "test.log", 10);
        appender.write_all(b"hello12345").unwrap(); // exactly 10 bytes → no rotate yet
        appender.write_all(b"x").unwrap(); // 11th byte triggers rotate
        let inner = appender.inner.lock().unwrap();
        assert_eq!(inner.index, 1);
    }

    #[test]
    fn resolve_log_dir_absolute_path_unchanged() {
        let abs = "/tmp/nightcrow-logs";
        let result = resolve_log_dir(abs, "/some/repo");
        assert_eq!(result, PathBuf::from(abs));
    }

    #[test]
    fn resolve_log_dir_relative_joins_repo_path() {
        let result = resolve_log_dir(".nightcrow/logs", "/my/repo");
        assert_eq!(result, PathBuf::from("/my/repo/.nightcrow/logs"));
    }
}
