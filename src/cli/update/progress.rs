//! The download's only sign of life.
//!
//! `update` spends the better part of a minute fetching a release binary and
//! printed nothing at all until it finished, which reads as a no-op.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

/// Fast enough to look live, slow enough not to flood a slow terminal.
const REDRAW: Duration = Duration::from_millis(100);

/// A log without carriage-return support gets one line per this many percent.
const LOG_STEP: u64 = 10;

pub(super) struct Progress {
    label: String,
    total: u64,
    interactive: bool,
    redrawn: Instant,
    reported: u64,
}

impl Progress {
    pub(super) fn start(label: &str, total: u64) -> Self {
        println!("nightcrow: downloading {label} ({})", human_bytes(total));
        let _ = std::io::stdout().flush();
        Self {
            label: label.to_owned(),
            total,
            interactive: std::io::stdout().is_terminal(),
            redrawn: Instant::now() - REDRAW,
            reported: 0,
        }
    }

    pub(super) fn advance(&mut self, downloaded: u64) {
        let percent = percent(downloaded, self.total);
        if self.interactive {
            if self.redrawn.elapsed() < REDRAW {
                return;
            }
            self.redrawn = Instant::now();
            print!("\r{}", status(percent, downloaded, self.total));
            let _ = std::io::stdout().flush();
        } else {
            if !logged(self.reported, percent) {
                return;
            }
            println!("{}", status(percent, downloaded, self.total));
        }
        self.reported = percent;
    }

    pub(super) fn finish(self) {
        if self.interactive {
            // Overwrite the partial line rather than leave it mid-percentage.
            print!("\r{}\n", status(100, self.total, self.total));
            let _ = std::io::stdout().flush();
        } else if self.reported < 100 {
            println!("{}", status(100, self.total, self.total));
        }
        println!("nightcrow: verifying {} checksum", self.label);
    }
}

fn percent(downloaded: u64, total: u64) -> u64 {
    if total == 0 {
        return 100;
    }
    (downloaded.min(total) * 100 / total).min(100)
}

fn logged(reported: u64, percent: u64) -> bool {
    percent / LOG_STEP > reported / LOG_STEP
}

fn status(percent: u64, downloaded: u64, total: u64) -> String {
    format!(
        "nightcrow: {percent:>3}% ({} / {})",
        human_bytes(downloaded.min(total)),
        human_bytes(total)
    )
}

fn human_bytes(bytes: u64) -> String {
    const MIB: f64 = (1024 * 1024) as f64;
    const KIB: f64 = 1024.0;
    let bytes = bytes as f64;
    if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{bytes:.0} B")
    }
}

#[cfg(test)]
#[path = "progress_tests.rs"]
mod tests;
