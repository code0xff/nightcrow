use super::*;
use crate::git::diff::{ChangedFile, StatusKind};

mod lifecycle;
mod publishing;
mod subscriptions;

const TEST_PAYLOAD_VERSION: u32 = 1;

fn encode_test_status(snapshot: &RepoSnapshot, _: &HashMap<String, SystemTime>) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "version": TEST_PAYLOAD_VERSION,
        "branch": snapshot.branch_name,
        "files": snapshot.files.iter().map(|file| &file.path).collect::<Vec<_>>(),
    }))
    .ok()
}

fn test_runtime() -> (Arc<RepoRuntime>, mpsc::Sender<SnapshotMsg>) {
    let (tx, rx) = mpsc::channel();
    let channel = SnapshotChannel::from_endpoints(rx);
    (
        RepoRuntime::start(channel, "test".to_string(), encode_test_status),
        tx,
    )
}

fn snapshot(branch: &str, files: usize) -> RepoSnapshot {
    RepoSnapshot {
        files: (0..files)
            .map(|i| {
                ChangedFile::from_status_columns(
                    format!("f{i}.rs"),
                    None,
                    StatusKind::Modified,
                    StatusKind::Unmodified,
                )
            })
            .collect(),
        tracking: None,
        head_oid: None,
        branch_name: Some(branch.to_string()),
        refs_fingerprint: 0,
    }
}

fn wait_for(mut check: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if check() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}
