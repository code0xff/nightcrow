use super::protocol::{DaemonStatus, RepositoryStatus, StatusUnavailable, StatusUnavailableReason};
use super::serve::Session;
use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Immutable process facts captured once when the attach session is created.
pub(super) struct DaemonMetadata {
    pid: u32,
    version: String,
    started_at: SystemTime,
    started_mono: Instant,
    web_endpoint: String,
    attach_endpoint: Result<String, StatusUnavailable>,
}

impl DaemonMetadata {
    pub(super) fn capture(attach_endpoint: &Path, web_addr: SocketAddr) -> Self {
        Self {
            pid: std::process::id(),
            version: super::protocol::version(),
            started_at: SystemTime::now(),
            started_mono: Instant::now(),
            web_endpoint: format!("http://{web_addr}/"),
            attach_endpoint: attach_endpoint
                .to_str()
                .map(str::to_owned)
                .ok_or(StatusUnavailable {
                    reason: StatusUnavailableReason::EndpointNotUnicode,
                }),
        }
    }

    pub(super) fn snapshot(&self, session: &Session) -> DaemonStatus {
        let repositories = session
            .state
            .status_snapshot()
            .into_iter()
            .map(|repo| RepositoryStatus {
                pane_count: repo.panes.len(),
                id: repo.id,
                path: repo.path,
                panes: repo.panes,
            })
            .collect();
        DaemonStatus {
            pid: self.pid,
            version: self.version.clone(),
            started_at_unix_ms: unix_millis(self.started_at),
            uptime_ms: millis(self.started_mono.elapsed()),
            web_endpoint: self.web_endpoint.clone(),
            attach_endpoint: self.attach_endpoint.clone(),
            repositories,
            attached_clients: session.clients.ids(),
        }
    }
}

fn unix_millis(time: SystemTime) -> Result<u64, StatusUnavailable> {
    time.duration_since(UNIX_EPOCH)
        .map(millis)
        .map_err(|_| StatusUnavailable {
            reason: StatusUnavailableReason::ClockBeforeUnixEpoch,
        })
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
