use crate::config::{LogConfig, LogRotation};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

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
            tracing_appender::non_blocking(appender)
        }
        LogRotation::Size => {
            let max_bytes = config.max_size_mb.saturating_mul(BYTES_PER_MB);
            if let Some(appender) = SizeRollingAppender::new(&log_dir, LOG_FILE_PREFIX, max_bytes) {
                tracing_appender::non_blocking(appender)
            } else {
                eprintln!(
                    "nightcrow: failed to open size-based log appender in {}; falling back to daily rotation",
                    log_dir.display()
                );
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

    // First pass: collect candidate files with mtimes so the newest one can
    // be preserved — SizeRollingAppender resumes its highest existing index
    // on startup, so the latest log file may itself be older than the
    // cutoff, and deleting it would lose the active session's tail.
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
/// Always preserves the newest entry, even if older than the cutoff —
/// SizeRollingAppender resumes the highest-index file, so deleting it would
/// drop the active session's tail. When two candidates share the maximum
/// mtime (1 s granularity on FAT/exFAT, simultaneous touches), only the first
/// is preserved; the others stay eligible so a tie doesn't inflate disk usage.
fn expired_log_paths(candidates: &[(PathBuf, SystemTime)], cutoff: SystemTime) -> Vec<&PathBuf> {
    let newest_idx = candidates
        .iter()
        .enumerate()
        .max_by_key(|(_, (_, t))| *t)
        .map(|(i, _)| i);
    candidates
        .iter()
        .enumerate()
        .filter(|(i, (_, t))| Some(*i) != newest_idx && *t < cutoff)
        .map(|(_, (p, _))| p)
        .collect()
}

fn is_nightcrow_log_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name == LOG_FILE_PREFIX {
        return true;
    }
    let Some(suffix) = name.strip_prefix(LOG_FILE_PREFIX_WITH_SEPARATOR) else {
        return false;
    };
    // Generated suffixes are size index (`0`, `12`, …), daily date
    // (`2026-05-03`), or hourly stamp (`2026-05-03-14`). Restricting to
    // digits + `-` keeps user-placed siblings like `nightcrow.log.backup`
    // from being swept up by cleanup_old_logs.
    !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit() || c == '-')
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
        .into_iter()
        .flatten()
        .flatten()
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
#[path = "logging_tests.rs"]
mod tests;
