//! The browser and attach socket share one clone slot and result registry.

use super::{ViewerServer, body_of, get, login, post, server};
use crate::daemon::client::DaemonClient;
use crate::daemon::frame::{Frame, FrameKind, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
use crate::daemon::socket::DaemonSocket;
use crate::daemon::transport::UnixStream;
use std::io::Write;
use std::path::Path;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Default)]
struct CloneGate {
    started: bool,
    released: bool,
}

static GATE: OnceLock<(Mutex<CloneGate>, Condvar)> = OnceLock::new();

fn gate() -> &'static (Mutex<CloneGate>, Condvar) {
    GATE.get_or_init(|| (Mutex::new(CloneGate::default()), Condvar::new()))
}

fn blocking_clone(_url: &str, destination: &Path) -> Result<(), String> {
    let (lock, wake) = gate();
    let mut state = lock.lock().unwrap();
    state.started = true;
    wake.notify_all();
    while !state.released {
        state = wake.wait(state).unwrap();
    }
    std::fs::write(destination.join("README.md"), "fake clone").map_err(|error| error.to_string())
}

struct ReleaseGate;

impl Drop for ReleaseGate {
    fn drop(&mut self) {
        let (lock, wake) = gate();
        lock.lock().unwrap().released = true;
        wake.notify_all();
    }
}

fn attach_daemon(dir: &tempfile::TempDir, web: &ViewerServer) -> DaemonSocket {
    let socket = DaemonSocket::bind(&dir.path().join("attach.sock")).expect("binds");
    let listener = socket.listener().try_clone().expect("clones");
    let (shutdown, _receiver) = std::sync::mpsc::sync_channel(1);
    let session =
        crate::daemon::serve::start(web.session_state(), socket.path(), web.addr(), shutdown)
            .expect("starts the watcher");
    std::thread::spawn(move || crate::daemon::serve::serve(listener, session));
    socket
}

fn send(stream: &mut UnixStream, message: &ClientMessage) {
    let payload = serde_json::to_vec(message).unwrap();
    write_frame(stream, &Frame::control(payload)).unwrap();
    stream.flush().unwrap();
}

fn next_control(stream: &mut UnixStream) -> ServerMessage {
    loop {
        let frame = read_frame(stream)
            .unwrap()
            .expect("daemon remains connected");
        if frame.kind == FrameKind::Control {
            return serde_json::from_slice(&frame.payload).unwrap();
        }
    }
}

fn await_job_status(client: &mut DaemonClient, job: u64) -> crate::session::CloneLookup {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        client.clone_status(Some(job)).unwrap();
        while Instant::now() < deadline {
            if let Some(ServerMessage::CloneStatus { lookup }) = client
                .drain()
                .into_iter()
                .find(|message| matches!(message, ServerMessage::CloneStatus { .. }))
            {
                if matches!(
                    lookup,
                    crate::session::CloneLookup::Found {
                        state: crate::session::CloneJobState::Running,
                        ..
                    }
                ) {
                    break;
                }
                return lookup;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(Instant::now() < deadline, "clone result did not return");
    }
}

#[test]
fn web_and_attach_share_admission_and_a_job_survives_attach_reconnect() {
    let (gate_lock, _) = gate();
    *gate_lock.lock().unwrap() = CloneGate::default();
    let _release = ReleaseGate;
    let web = server(&[]);
    web.session_state()
        .set_clone_runner_for_test(blocking_clone);
    let token = login(web.addr());
    let dir = tempfile::TempDir::new().unwrap();
    let parent = tempfile::TempDir::new().unwrap();
    let socket = attach_daemon(&dir, &web);
    let mut attach = UnixStream::connect(socket.path()).expect("connects");
    attach
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    send(&mut attach, &ClientMessage::Hello { version: version() });
    loop {
        if matches!(next_control(&mut attach), ServerMessage::Hello { .. }) {
            break;
        }
    }
    send(
        &mut attach,
        &ClientMessage::StartClone {
            parent: parent.path().to_string_lossy().into_owned(),
            url: "https://example.com/team/from-tui.git".into(),
        },
    );
    let job = loop {
        match next_control(&mut attach) {
            ServerMessage::CloneStarted { job, name } => {
                assert_eq!(name, "from-tui");
                break job;
            }
            ServerMessage::CloneRejected { message } => panic!("clone refused: {message}"),
            _ => {}
        }
    };
    let (lock, wake) = gate();
    let state = lock.lock().unwrap();
    let (state, result) = wake
        .wait_timeout_while(state, Duration::from_secs(5), |state| !state.started)
        .unwrap();
    assert!(!result.timed_out(), "worker did not enter the clone runner");
    drop(state);

    let conflict = post(
        web.addr(),
        "/api/clone",
        &serde_json::json!({
            "path": parent.path(),
            "url": "https://example.com/team/from-web.git",
        })
        .to_string(),
        Some(&token),
    );
    assert!(conflict.contains("409 Conflict"), "{conflict}");

    drop(attach);
    {
        let mut state = lock.lock().unwrap();
        state.released = true;
        wake.notify_all();
    }
    let mut reconnected = DaemonClient::connect(socket.path()).expect("reattaches");
    let status = await_job_status(&mut reconnected, job);
    assert_eq!(
        status,
        crate::session::CloneLookup::Found {
            job,
            state: crate::session::CloneJobState::Done {
                path: crate::platform::paths::for_display(
                    &parent.path().join("from-tui").canonicalize().unwrap(),
                )
                .into_owned(),
            },
        }
    );
    let status = get(web.addr(), &format!("/api/clone?job={job}"), Some(&token));
    let body = body_of(&status);
    assert!(body.contains("\"state\":\"done\""), "{body}");
}
