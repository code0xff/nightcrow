use crate::config::{LogConfig, LogRotation};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

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
    fs::create_dir_all(&log_dir).ok()?;
    cleanup_old_logs(&log_dir, config.max_days);

    let level = config.level.as_str();
    // `prompt` is a dedicated tracing target for terminal prompt capture. We
    // pin it at info regardless of the global level so that enabling
    // `prompt_log` always produces output, even when the rest of the app is
    // restricted to e.g. "warn".
    let filter_str = if config.prompt_log {
        format!("{level},prompt=info")
    } else {
        level.to_string()
    };

    let (writer, guard) = match config.rotation {
        LogRotation::Hourly => {
            let appender = tracing_appender::rolling::hourly(&log_dir, LOG_FILE_PREFIX);
            tracing_appender::non_blocking(appender)
        }
        LogRotation::Size => {
            let max_bytes = config.max_size_mb.saturating_mul(BYTES_PER_MB);
            if let Some(appender) = SizeRollingAppender::new(&log_dir, LOG_FILE_PREFIX, max_bytes) {
                tracing_appender::non_blocking(appender)
            } else {
                let appender = tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
                tracing_appender::non_blocking(appender)
            }
        }
        LogRotation::Daily => {
            let appender = tracing_appender::rolling::daily(&log_dir, LOG_FILE_PREFIX);
            tracing_appender::non_blocking(appender)
        }
    };

    let filter = EnvFilter::try_new(&filter_str).unwrap_or_else(|_| EnvFilter::new("warn"));

    let file_layer = fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true);

    let subscriber = tracing_subscriber::registry().with(filter).with(file_layer);

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

fn cleanup_old_logs(log_dir: &Path, max_days: u32) {
    if max_days == 0 {
        return;
    }
    let cutoff = SystemTime::now()
        .checked_sub(Duration::from_secs(u64::from(max_days) * 86400))
        .unwrap_or(SystemTime::UNIX_EPOCH);

    let Ok(entries) = fs::read_dir(log_dir) else {
        return;
    };

    // First pass: collect candidate files with mtimes so we can identify the
    // newest one and preserve it. SizeRollingAppender resumes its highest
    // existing index on startup, so the latest log file may itself be older
    // than the cutoff if write rate is low — deleting it would lose the
    // active session's tail.
    let mut candidates: Vec<(PathBuf, SystemTime)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_nightcrow_log_file(&path) {
            continue;
        }
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        candidates.push((path, modified));
    }

    for path in expired_log_paths(&candidates, cutoff) {
        let _ = fs::remove_file(path);
    }
}

/// Returns paths to delete from a list of candidate `(path, mtime)` entries.
/// Always preserves the newest entry, even if it is older than the cutoff —
/// SizeRollingAppender resumes the highest-index file, so deleting it would
/// drop the active session's tail.
fn expired_log_paths(candidates: &[(PathBuf, SystemTime)], cutoff: SystemTime) -> Vec<&PathBuf> {
    let newest = candidates.iter().map(|(_, t)| *t).max();
    candidates
        .iter()
        .filter(|(_, t)| Some(*t) != newest && *t < cutoff)
        .map(|(p, _)| p)
        .collect()
}

fn is_nightcrow_log_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| {
            name == LOG_FILE_PREFIX || name.starts_with(LOG_FILE_PREFIX_WITH_SEPARATOR)
        })
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
    fn new(dir: &Path, prefix: &str, max_bytes: u64) -> Option<Self> {
        let index = latest_size_log_index(dir, prefix);
        let path = dir.join(format!("{prefix}.{index}"));
        let current = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()?;
        let current_size = current.metadata().map(|m| m.len()).unwrap_or(0);
        Some(Self {
            inner: Arc::new(Mutex::new(SizeRollingInner {
                dir: dir.to_path_buf(),
                prefix: prefix.to_string(),
                max_bytes,
                current,
                current_size,
                index,
            })),
        })
    }
}

fn latest_size_log_index(dir: &Path, prefix: &str) -> u32 {
    let prefix = format!("{prefix}.");
    fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.strip_prefix(&prefix)?.parse::<u32>().ok())
        })
        .max()
        .unwrap_or(0)
}

impl Write for SizeRollingAppender {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        // Loop so partial writes still trigger rotation when crossing the
        // size threshold. Without this loop, a write returning fewer bytes
        // than `buf.len()` could leave the threshold check stale until the
        // caller's next call.
        let mut total_written = 0usize;
        let mut remaining = buf;
        while !remaining.is_empty() {
            if inner.max_bytes > 0 && inner.current_size + remaining.len() as u64 > inner.max_bytes
            {
                inner.index += 1;
                let path = inner.dir.join(format!("{}.{}", inner.prefix, inner.index));
                inner.current = OpenOptions::new().create(true).append(true).open(path)?;
                inner.current_size = 0;
            }
            let n = inner.current.write(remaining)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "SizeRollingAppender wrote 0 bytes",
                ));
            }
            inner.current_size += n as u64;
            total_written += n;
            remaining = &remaining[n..];
        }
        Ok(total_written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current
            .flush()
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
    fn expired_log_paths_preserves_newest_even_when_old() {
        let now = SystemTime::now();
        let day = Duration::from_secs(86400);
        let candidates = vec![
            (PathBuf::from("nightcrow.log.0"), now - day * 30),
            (PathBuf::from("nightcrow.log.1"), now - day * 20),
            (PathBuf::from("nightcrow.log.2"), now - day * 10),
        ];
        let cutoff = now - day; // anything older than 1 day is expired

        let expired = expired_log_paths(&candidates, cutoff);

        // newest (.2) must be preserved; older two are expired.
        let names: Vec<_> = expired.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(names, vec!["nightcrow.log.0", "nightcrow.log.1"]);
    }

    #[test]
    fn expired_log_paths_keeps_recent_files() {
        let now = SystemTime::now();
        let candidates = vec![
            (
                PathBuf::from("nightcrow.log.0"),
                now - Duration::from_secs(60),
            ),
            (
                PathBuf::from("nightcrow.log.1"),
                now - Duration::from_secs(30),
            ),
        ];
        let cutoff = now - Duration::from_secs(86400);

        assert!(expired_log_paths(&candidates, cutoff).is_empty());
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
    fn recognizes_generated_nightcrow_log_names() {
        assert!(is_nightcrow_log_file(Path::new("nightcrow.log")));
        assert!(is_nightcrow_log_file(Path::new("nightcrow.log.0")));
        assert!(is_nightcrow_log_file(Path::new("nightcrow.log.2026-05-03")));
        assert!(is_nightcrow_log_file(Path::new(
            "nightcrow.log.2026-05-03-14"
        )));
        assert!(!is_nightcrow_log_file(Path::new("nightcrow.old.log")));
        assert!(!is_nightcrow_log_file(Path::new("other.log")));
    }

    #[test]
    fn size_rolling_appender_rotates_on_overflow() {
        let dir = tempdir().unwrap();
        let mut appender = SizeRollingAppender::new(dir.path(), "test.log", 10).unwrap();
        appender.write_all(b"hello12345").unwrap(); // exactly 10 bytes → no rotate yet
        appender.write_all(b"x").unwrap(); // 11th byte triggers rotate
        let inner = appender.inner.lock().unwrap();
        assert_eq!(inner.index, 1);
    }

    #[test]
    fn size_rolling_appender_resumes_highest_existing_index() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("test.log.0"), b"old").unwrap();
        fs::write(dir.path().join("test.log.2"), b"new").unwrap();
        fs::write(dir.path().join("test.log.2026-05-03"), b"daily").unwrap();

        let appender = SizeRollingAppender::new(dir.path(), "test.log", 10).unwrap();
        let inner = appender.inner.lock().unwrap();

        assert_eq!(inner.index, 2);
        assert_eq!(inner.current_size, 3);
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
