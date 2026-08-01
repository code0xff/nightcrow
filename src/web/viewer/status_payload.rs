//! Browser status-payload encoding injected into the shared session runtime.

use crate::git::diff::RepoSnapshot;
use crate::web::viewer::dto::{Envelope, StatusDto};
use std::collections::HashMap;
use std::time::SystemTime;

pub(crate) fn encode(
    snapshot: &RepoSnapshot,
    mtimes: &HashMap<String, SystemTime>,
) -> Option<String> {
    let dto = StatusDto::from_snapshot(
        &snapshot.files,
        snapshot.tracking.as_ref(),
        snapshot.head_oid,
        snapshot.branch_name.as_deref(),
        mtimes,
    );
    let json = match serde_json::to_string(&Envelope::new(dto)) {
        Ok(json) => json,
        Err(err) => {
            tracing::warn!(%err, "viewer: status payload failed to serialize");
            return None;
        }
    };
    if json.len() > crate::web::viewer::limits::MAX_SSE_PAYLOAD_BYTES {
        tracing::warn!(bytes = json.len(), "viewer: status payload over ceiling");
        return None;
    }
    Some(json)
}
