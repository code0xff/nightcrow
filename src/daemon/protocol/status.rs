use crate::backend::PaneId;
use serde::{Deserialize, Serialize};

/// Authoritative runtime facts owned by the daemon and its session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonStatus {
    pub pid: u32,
    pub version: String,
    pub started_at_unix_ms: Result<u64, StatusUnavailable>,
    pub uptime_ms: u64,
    /// The HTTP endpoint the viewer listener bound at runtime.
    pub web_endpoint: String,
    /// The attach socket endpoint, or why its path could not be represented as
    /// text.
    pub attach_endpoint: Result<String, StatusUnavailable>,
    pub repositories: Vec<RepositoryStatus>,
    /// Attach protocol client ids only. Terminal-hub connection ids are a
    /// different namespace and deliberately do not appear here.
    pub attached_clients: Vec<u64>,
}

/// One open repository and the panes its daemon-owned hub currently owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryStatus {
    pub id: String,
    pub path: String,
    pub panes: Vec<PaneId>,
    pub pane_count: usize,
}

/// A value the daemon could not represent, with a machine-readable reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusUnavailable {
    pub reason: StatusUnavailableReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusUnavailableReason {
    ClockBeforeUnixEpoch,
    EndpointNotUnicode,
}
