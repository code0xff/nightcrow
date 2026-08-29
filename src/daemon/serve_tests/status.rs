use super::harness::*;
use crate::daemon::frame::{Frame, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
use crate::daemon::transport::UnixStream;
use std::io::Write;

fn one_shot(path: &std::path::Path, frame: Frame) -> (ServerMessage, UnixStream) {
    let mut stream = UnixStream::connect(path).expect("connects");
    write_frame(&mut stream, &frame).expect("writes first frame");
    stream.flush().expect("flushes first frame");
    let frame = read_frame(&mut stream)
        .expect("reads response")
        .expect("daemon responds");
    let message = serde_json::from_slice(&frame.payload).expect("response decodes");
    (message, stream)
}

fn status_frame() -> Frame {
    Frame::control(serde_json::to_vec(&ClientMessage::Status {}).unwrap())
}

#[test]
fn status_is_authoritative_and_does_not_attach_or_mutate_the_session() {
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut attached = Client::attach(daemon.path());
    let attached_id = attached.hello();

    let before = diagnostics(&daemon);
    let (answer, mut stream) = one_shot(daemon.path(), status_frame());
    let ServerMessage::Status { status } = answer else {
        panic!("expected status response, got {answer:?}");
    };

    assert_eq!(status.pid, std::process::id());
    assert_eq!(status.version, version());
    assert_eq!(
        status.endpoint.as_deref(),
        Ok(daemon.path().to_str().expect("test path is Unicode"))
    );
    assert_eq!(status.attached_clients, vec![attached_id]);
    assert_eq!(status.repositories.len(), 1);
    assert_eq!(status.repositories[0].path, resolved(&path));
    assert_eq!(
        status.repositories[0].pane_count,
        status.repositories[0].panes.len()
    );
    assert!(status.started_at_unix_ms.is_ok());
    assert_eq!(diagnostics(&daemon), before);
    assert!(read_frame(&mut stream).expect("clean close").is_none());
    drop(repo);
}

#[test]
fn a_non_handshake_first_request_is_refused_without_attaching() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let frame = Frame::control(serde_json::to_vec(&ClientMessage::ListRepos).unwrap());

    let (answer, _) = one_shot(daemon.path(), frame);

    assert!(matches!(answer, ServerMessage::Error { .. }));
    assert_eq!(daemon.session.clients.len(), 0);
    assert!(daemon.session.bridges.lock().unwrap().is_empty());
}

#[test]
fn an_invalid_first_request_is_refused_without_attaching() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);

    let (answer, _) = one_shot(daemon.path(), Frame::control(b"{not json".to_vec()));

    assert!(matches!(answer, ServerMessage::Error { .. }));
    assert_eq!(daemon.session.clients.len(), 0);
}

#[test]
fn stop_request_is_still_accepted_before_attach() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);

    crate::cli::run_stop(Some(daemon.path().to_path_buf())).expect("stop request succeeds");

    assert_eq!(
        daemon
            .shutdown_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .expect("the daemon receives the stop signal"),
        crate::platform::signals::Shutdown::Terminate
    );
    assert_eq!(daemon.session.clients.len(), 0);
    assert!(daemon.session.bridges.lock().unwrap().is_empty());
}

#[test]
fn matching_hello_transitions_from_pre_attach_to_one_attached_client() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach_raw(daemon.path());

    client.hello();

    assert_eq!(daemon.session.pre_attach_active(), 0);
    assert_eq!(daemon.session.clients.len(), 1);
}

#[test]
fn attached_cap_rejection_returns_the_pre_attach_permit() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut clients = Vec::new();
    for _ in 0..crate::daemon::serve::MAX_ATTACHED_CLIENTS {
        clients.push(Client::attach(daemon.path()));
    }
    assert_eq!(
        daemon.session.clients.len(),
        crate::daemon::serve::MAX_ATTACHED_CLIENTS
    );
    assert_eq!(daemon.session.pre_attach_active(), 0);

    let mut rejected = Client::attach_raw(daemon.path());
    rejected.send(ClientMessage::Hello { version: version() });
    rejected
        .stream
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .expect("sets a timeout");
    assert!(
        read_frame(&mut rejected.stream)
            .expect("the capped connection closes")
            .is_none()
    );
    assert_eq!(daemon.session.pre_attach_active(), 0);
    assert_eq!(
        daemon.session.clients.len(),
        crate::daemon::serve::MAX_ATTACHED_CLIENTS
    );
}

#[derive(Debug, PartialEq, Eq)]
struct Diagnostics {
    attached: usize,
    bridges: usize,
    terminal_clients: usize,
    repositories: Vec<crate::session::RepositoryStatusSnapshot>,
    active: Option<String>,
    accent: usize,
}

fn diagnostics(daemon: &TestDaemon) -> Diagnostics {
    let entries = daemon.state().catalog().entries();
    Diagnostics {
        attached: daemon.session.clients.len(),
        bridges: daemon.session.bridges.lock().unwrap().len(),
        terminal_clients: entries
            .iter()
            .map(|entry| entry.terminals.client_count())
            .sum(),
        repositories: daemon.state().status_snapshot(),
        active: crate::session::active_repo(daemon.state()),
        accent: crate::session::accent(daemon.state()),
    }
}
