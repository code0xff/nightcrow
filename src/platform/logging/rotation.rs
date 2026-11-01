//! Log files that rotate on a schedule or a size, and the retention sweep that
//! runs when they do.
//!
//! Retention is otherwise only applied at startup, which never happens again in
//! a daemon that stays up across rotations.

use super::{LOG_FILE_PREFIX, LOG_FILE_PREFIX_WITH_SEPARATOR};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

pub(super) fn cleanup_old_logs(log_dir: &Path, max_days: u32) {
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

/// A time-rotated appender that sweeps expired logs when its period rolls over.
///
/// `tracing_appender` rotates inside its own `write`, on UTC hour and day
/// boundaries, so there is no rotation event to hook. The period bucket is the
/// observable equivalent. The sweep runs before the inner write, and preserves
/// the newest file — the one the appender is about to leave behind.
pub(super) struct TimeRotationCleanup<W> {
    inner: W,
    dir: PathBuf,
    max_days: u32,
    period_secs: u64,
    bucket: u64,
}

impl<W> TimeRotationCleanup<W> {
    pub(super) fn new(inner: W, dir: &Path, max_days: u32, period_secs: u64) -> Self {
        Self {
            inner,
            dir: dir.to_path_buf(),
            max_days,
            period_secs,
            bucket: epoch_secs() / period_secs,
        }
    }

    pub(super) fn rotate_if_due(&mut self, now_secs: u64) {
        let bucket = now_secs / self.period_secs;
        if bucket != self.bucket {
            self.bucket = bucket;
            cleanup_old_logs(&self.dir, self.max_days);
        }
    }
}

impl<W: Write> Write for TimeRotationCleanup<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.rotate_if_due(epoch_secs());
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub(super) fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

/// Returns paths to delete from a list of candidate `(path, mtime)` entries.
/// Always preserves the newest entry, even if older than the cutoff —
/// SizeRollingAppender resumes the highest-index file, so deleting it would
/// drop the active session's tail. When two candidates share the maximum
/// mtime (1 s granularity on FAT/exFAT, simultaneous touches), only the first
/// is preserved; the others stay eligible so a tie doesn't inflate disk usage.
pub(super) fn expired_log_paths(
    candidates: &[(PathBuf, SystemTime)],
    cutoff: SystemTime,
) -> Vec<&PathBuf> {
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

pub(super) fn is_nightcrow_log_file(path: &Path) -> bool {
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
pub(super) struct SizeRollingAppender {
    // Accessed by the logging tests.
    pub(super) inner: Arc<Mutex<SizeRollingInner>>,
}

pub(super) struct SizeRollingInner {
    dir: PathBuf,
    prefix: String,
    max_bytes: u64,
    max_days: u32,
    current: File,
    pub(super) current_size: u64,
    pub(super) index: u32,
}

impl SizeRollingAppender {
    pub(super) fn new(dir: &Path, prefix: &str, max_bytes: u64, max_days: u32) -> Option<Self> {
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
                max_days,
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
        let mut rotated = false;
        while !remaining.is_empty() {
            if inner.max_bytes > 0 && inner.current_size + remaining.len() as u64 > inner.max_bytes
            {
                inner.index += 1;
                let path = inner.dir.join(format!("{}.{}", inner.prefix, inner.index));
                inner.current = OpenOptions::new().create(true).append(true).open(path)?;
                inner.current_size = 0;
                rotated = true;
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
        // The sweep reads the directory and can remove files, so it runs after
        // releasing the lock the writers share.
        let sweep = rotated.then(|| (inner.dir.clone(), inner.max_days));
        drop(inner);
        if let Some((dir, max_days)) = sweep {
            cleanup_old_logs(&dir, max_days);
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
