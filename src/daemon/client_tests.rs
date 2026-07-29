use super::DaemonClient;
use crate::daemon::protocol::ServerMessage;
use crate::daemon::socket::DaemonSocket;
use crate::web::common::auth::Auth;
use crate::web::viewer::prefs::PrefsStore;
use crate::web::viewer::server::{ViewerOptions, ViewerState};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A running daemon, held so its socket stays bound for the test.
struct TestDaemon {
    socket: DaemonSocket,
}

impl TestDaemon {
    fn path(&self) -> &std::path::Path {
        self.socket.path()
    }
}

fn daemon(dir: &tempfile::TempDir, repos: &[String]) -> TestDaemon {
    let path = dir.path().join("d.sock");
    let socket = DaemonSocket::bind(&path).expect("binds");
    let listener = socket.listener().try_clone().expect("clones");
    let state = Arc::new(ViewerState::new(ViewerOptions {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        auth: Auth::from_plaintext("swordfish").unwrap(),
        repos: repos.to_vec(),
        persist: false,
        startup_commands: Vec::new(),
        hot: crate::config::AgentIndicatorConfig::default(),
        prefs: PrefsStore::at(std::path::PathBuf::from(
            "/nonexistent/nightcrow/viewer.json",
        )),
    }));
    std::thread::spawn(move || crate::daemon::serve::serve(listener, state));
    TestDaemon { socket }
}

/// Drain until a repository set arrives, or give up.
///
/// Messages cross a socket and a thread, so "not yet" is normal and only a
/// deadline distinguishes it from "never". The deadline is generous because a
/// slow machine must not fail the test; the loop exits as soon as it can.
fn await_repos(client: &mut DaemonClient) -> Vec<String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        for message in client.drain() {
            if let ServerMessage::Repos { repos } = message {
                return repos.into_iter().map(|repo| repo.path).collect();
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no repository set arrived");
}

#[test]
fn attaching_completes_the_handshake_and_keeps_the_volunteered_set() {
    // The daemon sends the repository set on attach, so it can arrive before
    // the handshake answer. Dropping it would leave the client with nothing to
    // render until something else changed.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));

    let mut client = DaemonClient::connect(daemon.path()).expect("attaches");

    assert_eq!(await_repos(&mut client), vec![path]);
    assert!(client.is_connected());
    drop(repo);
}

#[test]
fn attaching_where_no_daemon_listens_says_so() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("absent.sock");

    let err = DaemonClient::connect(&path).expect_err("there is nothing to attach to");

    assert!(
        err.to_string().contains("no nightcrow daemon"),
        "the error should point at the fix: {err}"
    );
}

#[test]
fn opening_a_repository_comes_back_as_a_broadcast() {
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = DaemonClient::connect(daemon.path()).expect("attaches");
    assert!(await_repos(&mut client).is_empty(), "starts empty");

    client.open_repo(&path).expect("sends");

    let resolved = crate::git::resolve_repo_path(std::path::Path::new(&path))
        .to_string_lossy()
        .into_owned();
    assert_eq!(await_repos(&mut client), vec![resolved]);
    drop(repo);
}

#[test]
fn a_change_by_another_client_arrives_unprompted() {
    // Nothing on this client asked. This is what rules out a request/response
    // client and is why `drain` exists.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut watcher = DaemonClient::connect(daemon.path()).expect("attaches");
    let mut actor = DaemonClient::connect(daemon.path()).expect("attaches");
    assert!(await_repos(&mut watcher).is_empty(), "starts empty");

    actor.open_repo(&path).expect("sends");

    assert_eq!(await_repos(&mut watcher).len(), 1);
    drop(repo);
}

#[test]
fn draining_an_idle_session_returns_nothing_and_does_not_block() {
    // Every frame calls this. A quiet daemon is the normal state, so it must
    // cost nothing rather than wait.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = DaemonClient::connect(daemon.path()).expect("attaches");
    await_repos(&mut client);

    let started = Instant::now();
    let messages = client.drain();

    assert!(messages.is_empty());
    assert!(
        started.elapsed() < Duration::from_millis(100),
        "drain must not wait on the socket"
    );
}

#[test]
fn a_refusal_is_delivered_rather_than_dropped() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = DaemonClient::connect(daemon.path()).expect("attaches");
    await_repos(&mut client);

    client.open_repo("/no/such/place").expect("sends");

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        for message in client.drain() {
            if let ServerMessage::Error { message } = message {
                assert!(message.contains("no such directory"), "{message}");
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the refusal never arrived");
}

#[test]
fn a_daemon_that_goes_away_is_noticed_without_losing_what_it_said() {
    // The reason connection state is a flag and not the channel's: a client
    // that asks "are you still there" must not consume the answer to its last
    // question in the process.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = DaemonClient::connect(daemon.path()).expect("attaches");
    // Let the volunteered set land in the queue, then take the daemon away
    // without draining it.
    std::thread::sleep(Duration::from_millis(50));
    drop(daemon);
    // The socket file is gone, but this connection is still open until the
    // daemon's threads end with the process — so the client is still attached.
    assert!(client.is_connected());
    assert_eq!(
        client
            .drain()
            .into_iter()
            .filter(|m| matches!(m, ServerMessage::Repos { .. }))
            .count(),
        1,
        "the set the daemon volunteered survives"
    );
    drop(repo);
}
