//! Wire-level exercise of requester-only clone start and polling replies.

use crate::daemon::client::DaemonClient;
use crate::daemon::frame::{Frame, FrameKind, read_frame, write_frame};
use crate::daemon::protocol::{ServerMessage, version};
use crate::daemon::socket::DaemonSocket;
use crate::daemon::transport::UnixListener;
use std::io::Write;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

fn daemon(dir: &tempfile::TempDir) -> DaemonSocket {
    let socket = DaemonSocket::bind(&dir.path().join("clone.sock")).expect("binds");
    let listener = socket.listener().try_clone().expect("clones");
    let state = Arc::new(crate::session::SessionState::new(
        crate::session::SessionOptions {
            repos: Vec::new(),
            persist: false,
            startup_commands: Vec::new(),
            cli_startup: Vec::new(),
            terminal: crate::config::TerminalConfig::default(),
            shell: crate::config::ShellConfig::default(),
            prefs: crate::session::prefs::PrefsStore::at(dir.path().join("viewer.json")),
            status_encoder: crate::session::test_status_encoder,
        },
    ));
    let (shutdown, _receiver) = std::sync::mpsc::sync_channel(1);
    let session = crate::daemon::serve::start(
        state,
        socket.path(),
        "127.0.0.1:4321".parse().unwrap(),
        shutdown,
    )
    .expect("starts the watcher");
    std::thread::spawn(move || crate::daemon::serve::serve(listener, session));
    socket
}

fn await_clone_reply(client: &mut DaemonClient) -> ServerMessage {
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if let Some(reply) = client.drain().into_iter().find(|message| {
            matches!(
                message,
                ServerMessage::CloneStarted { .. }
                    | ServerMessage::CloneStatus { .. }
                    | ServerMessage::CloneRejected { .. }
            )
        }) {
            return reply;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("no typed clone reply arrived");
}

fn await_initial_repos(client: &mut DaemonClient) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if client
            .drain()
            .iter()
            .any(|message| matches!(message, ServerMessage::Repos { .. }))
        {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no initial repo set arrived");
}

#[test]
fn a_legacy_hello_disables_clone_and_emits_no_new_protocol_requests() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("legacy.sock");
    let listener = UnixListener::bind(&path).expect("binds");
    let (ready_tx, ready_rx) = mpsc::channel();
    let (continue_tx, continue_rx) = mpsc::channel();
    let (request_tx, request_rx) = mpsc::channel();
    let peer = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accepts");
        let hello = read_frame(&mut stream).unwrap().expect("client hello");
        assert_eq!(hello.kind, FrameKind::Control);
        let response = serde_json::to_vec(&serde_json::json!({
            "type": "hello",
            "version": version(),
            "client": 4,
        }))
        .unwrap();
        write_frame(&mut stream, &Frame::control(response)).unwrap();
        stream.flush().unwrap();
        ready_tx.send(()).unwrap();
        continue_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .unwrap();
        let request = read_frame(&mut stream).ok().flatten();
        request_tx.send(request.is_none()).unwrap();
    });

    let mut client = DaemonClient::connect(&path).expect("legacy hello is accepted");
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(!client.can_clone());
    assert!(
        client
            .start_clone("/tmp", "https://example.com/team/repo.git")
            .is_err()
    );
    assert!(client.clone_status(None).is_err());
    continue_tx.send(()).unwrap();
    assert!(request_rx.recv_timeout(Duration::from_secs(5)).unwrap());
    drop(client);
    peer.join().unwrap();
}

#[test]
fn clone_start_and_status_use_typed_requester_only_daemon_replies() {
    let dir = tempfile::TempDir::new().unwrap();
    let parent = tempfile::TempDir::new().unwrap();
    let socket = daemon(&dir);
    let mut requester = DaemonClient::connect(socket.path()).expect("attaches");
    let mut observer = DaemonClient::connect(socket.path()).expect("attaches");
    await_initial_repos(&mut requester);
    await_initial_repos(&mut observer);
    assert_eq!(requester.can_clone(), crate::git::clone::git_available());

    requester
        .start_clone(
            parent.path().to_str().unwrap(),
            "https://127.0.0.1:1/team/thing.git",
        )
        .expect("sends clone request");
    let job = match await_clone_reply(&mut requester) {
        ServerMessage::CloneStarted { job, name } => {
            assert_eq!(name, "thing");
            job
        }
        ServerMessage::CloneRejected { message } if !crate::git::clone::git_available() => {
            assert!(message.contains("git"));
            return;
        }
        other => panic!("expected typed start reply, got {other:?}"),
    };
    assert!(observer.drain().into_iter().all(|message| !matches!(
        message,
        ServerMessage::CloneStarted { .. }
            | ServerMessage::CloneStatus { .. }
            | ServerMessage::CloneRejected { .. }
    )));

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        requester
            .clone_status(Some(job))
            .expect("sends status request");
        match await_clone_reply(&mut requester) {
            ServerMessage::CloneStatus {
                lookup:
                    crate::session::CloneLookup::Found {
                        job: reported,
                        state: crate::session::CloneJobState::Running,
                    },
            } => {
                assert_eq!(reported, job);
                assert!(Instant::now() < deadline, "git did not finish");
                std::thread::sleep(Duration::from_millis(40));
            }
            ServerMessage::CloneStatus {
                lookup:
                    crate::session::CloneLookup::Found {
                        job: reported,
                        state: crate::session::CloneJobState::Failed { message },
                    },
            } => {
                assert_eq!(reported, job);
                assert!(!message.is_empty());
                break;
            }
            other => panic!("expected typed job status, got {other:?}"),
        }
    }
    assert!(!parent.path().join("thing").exists());
}
