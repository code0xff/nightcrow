use super::*;
use std::cell::RefCell;
use std::path::Path;

#[test]
fn preapproved_restart_needs_no_terminal_io_and_runs_once() {
    let dir = tempfile::TempDir::new().expect("temp dir");
    let socket = dir.path().join("live.sock");
    let log = dir.path().join("daemon.out");
    let events = RefCell::new(Vec::new());

    let result = restart_for_mismatch(
        RestartContext {
            socket: &socket,
            log: &log,
            original_error: anyhow::anyhow!("client is 0.1.1, daemon is 0.1.2"),
            mismatch: VersionMismatch::new("0.1.1".into(), "0.1.2".into()),
        },
        RestartPrompt::preapproved(),
        RestartOperations {
            stop: |path: &Path| {
                assert_eq!(path, socket.as_path());
                events.borrow_mut().push("stop");
                Ok(())
            },
            start: |path: &Path| {
                assert_eq!(path, log.as_path());
                events.borrow_mut().push("start");
                Ok(())
            },
            wait: |_: &mut (), path: &Path, _: &Path, _: anyhow::Error| -> anyhow::Result<()> {
                assert_eq!(path, socket.as_path());
                events.borrow_mut().push("attach");
                Ok(())
            },
        },
    );

    result.expect("the replacement session attaches");
    assert_eq!(*events.borrow(), ["stop", "start", "attach"]);
}
