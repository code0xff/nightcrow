use super::super::frame::{Frame, FrameKind, read_frame, write_frame};
use super::super::protocol::{ClientMessage, ServerMessage, TerminalOutput, version};
use super::super::socket::DaemonSocket;
use crate::backend::PaneId;
use crate::web::common::auth::Auth;
use crate::web::viewer::prefs::PrefsStore;
use crate::web::viewer::server::{ViewerOptions, ViewerState};
use crate::web::viewer::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage,
};
use std::io::Write;
use std::os::unix::net::UnixStream;
use std::sync::Arc;

/// A running daemon. Held by the test so its socket stays bound and its
/// instance lock stays taken for the duration.
struct TestDaemon {
    socket: DaemonSocket,
}

impl TestDaemon {
    fn path(&self) -> &std::path::Path {
        self.socket.path()
    }
}

/// A daemon serving `repos`, on its own socket in `dir`.
///
/// No TCP port is taken — the session exists without a browser listener, which
/// is the point of building the state separately.
fn daemon(dir: &tempfile::TempDir, repos: &[String]) -> TestDaemon {
    let path = dir.path().join("d.sock");
    let socket = DaemonSocket::bind(&path).expect("binds");
    let listener = socket.listener().try_clone().expect("clones");
    let state = Arc::new(ViewerState::new(ViewerOptions {
        bind: "127.0.0.1".parse().unwrap(),
        port: 0,
        auth: Auth::from_plaintext("swordfish").unwrap(),
        repos: repos.to_vec(),
        // Never persist from tests — they must not touch the real
        // ~/.nightcrow/workspace.json.
        persist: false,
        startup_commands: Vec::new(),
        hot: crate::config::AgentIndicatorConfig::default(),
        prefs: PrefsStore::at(std::path::PathBuf::from(
            "/nonexistent/nightcrow/viewer.json",
        )),
    }));
    std::thread::spawn(move || super::serve(listener, state));
    TestDaemon { socket }
}

/// A client attached to the daemon at `path`.
struct Client {
    stream: UnixStream,
}

impl Client {
    /// Attach and consume the repository set the daemon sends unprompted.
    fn attach(path: &std::path::Path) -> Self {
        let mut client = Self {
            stream: UnixStream::connect(path).expect("attaches"),
        };
        client.next_repos();
        client
    }

    /// Attach without consuming anything, for tests about the first frame.
    fn attach_raw(path: &std::path::Path) -> Self {
        Self {
            stream: UnixStream::connect(path).expect("attaches"),
        }
    }

    fn send(&mut self, message: ClientMessage) {
        let json = serde_json::to_vec(&message).expect("encodes");
        write_frame(&mut self.stream, &Frame::control(json)).expect("writes");
        self.stream.flush().expect("flushes");
    }

    /// The next message from the daemon, whether it answers a request of this
    /// client's or reports a change another one made.
    fn next(&mut self) -> ServerMessage {
        self.stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("sets a timeout");
        let frame = read_frame(&mut self.stream)
            .expect("reads")
            .expect("the daemon speaks");
        assert_eq!(frame.kind, FrameKind::Control);
        serde_json::from_slice(&frame.payload).expect("decodes")
    }

    fn ask(&mut self, message: ClientMessage) -> ServerMessage {
        self.send(message);
        self.next()
    }

    /// Complete the handshake and return the id the daemon knows this
    /// connection by.
    fn hello(&mut self) -> u64 {
        self.send(ClientMessage::Hello { version: version() });
        for _ in 0..64 {
            if let ServerMessage::Hello { client, .. } = self.next() {
                return client;
            }
        }
        panic!("no hello arrived among the session traffic");
    }

    /// The next pane the daemon reports, and who it says asked for it.
    fn next_created(&mut self) -> (PaneId, Option<u64>) {
        for _ in 0..64 {
            if let (
                _,
                HubServerMessage::Created {
                    pane,
                    client: requester,
                    ..
                },
            ) = self.next_terminal_event()
            {
                return (pane, requester);
            }
        }
        panic!("no pane was created");
    }

    /// The next terminal event for any repository, skipping the tab list.
    fn next_terminal_event(&mut self) -> (String, HubServerMessage) {
        for _ in 0..64 {
            let frame = read_frame(&mut self.stream)
                .expect("reads")
                .expect("the daemon speaks");
            if frame.kind != FrameKind::Control {
                continue;
            }
            if let Ok(ServerMessage::Terminal { repo, event }) =
                serde_json::from_slice(&frame.payload)
            {
                return (repo, event);
            }
        }
        panic!("no terminal event arrived");
    }

    /// The next chunk of pane output.
    fn next_output(&mut self) -> TerminalOutput {
        for _ in 0..512 {
            self.stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .expect("sets a timeout");
            let frame = read_frame(&mut self.stream)
                .expect("reads")
                .expect("the daemon speaks");
            if frame.kind == FrameKind::Terminal {
                return TerminalOutput::decode(&frame.payload).expect("a well-formed output frame");
            }
        }
        panic!("no pane output arrived");
    }

    /// The catalog ids of the open repositories, asked for rather than waited
    /// for: the set the daemon volunteers on attach may already have been read
    /// past by a test that was after something else.
    fn repo_ids(&mut self) -> Vec<String> {
        self.send(ClientMessage::ListRepos);
        match self.next_repos() {
            ServerMessage::Repos { repos } => repos.into_iter().map(|repo| repo.id).collect(),
            other => panic!("expected a repo list, got {other:?}"),
        }
    }

    /// The next repository set, skipping terminal traffic.
    ///
    /// Subscribing a client to its repositories starts that traffic
    /// immediately — a hub with startup terminals offers them to be sized
    /// before anything else happens — so a test about the tab list has to read
    /// past it rather than assume the next frame is the one it wants.
    fn next_repos(&mut self) -> ServerMessage {
        for _ in 0..64 {
            let message = self.next();
            if matches!(message, ServerMessage::Repos { .. }) {
                return message;
            }
        }
        panic!("no repository set arrived among the terminal traffic");
    }
}

/// The path the catalog stores for `path`: the worktree root git resolves it
/// to. Both `--repo` and an open request are reduced to this before the catalog
/// sees them, so two spellings of one repository collapse to one entry.
fn resolved(path: &str) -> String {
    crate::git::resolve_repo_path(std::path::Path::new(path))
        .to_string_lossy()
        .into_owned()
}

fn repo_paths(answer: &ServerMessage) -> Vec<String> {
    match answer {
        ServerMessage::Repos { repos } => repos.iter().map(|r| r.path.clone()).collect(),
        other => panic!("expected a repo list, got {other:?}"),
    }
}

#[test]
fn a_client_that_says_hello_is_answered_with_the_daemon_version() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    let answer = client.ask(ClientMessage::Hello { version: version() });

    match answer {
        ServerMessage::Hello {
            version: daemon, ..
        } => assert_eq!(daemon, version()),
        other => panic!("expected a hello, got {other:?}"),
    }
}

#[test]
fn each_client_is_told_the_id_the_daemon_knows_it_by() {
    // It is how a client recognises a pane it asked for among the ones every
    // client is told about, so two attachments must not share one.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut first = Client::attach(daemon.path());
    let mut second = Client::attach(daemon.path());

    assert_ne!(first.hello(), second.hello());
}

#[test]
fn a_version_mismatch_is_reported_rather_than_ignored() {
    // Two builds running at once. Saying so beats failing later on a message
    // one side cannot decode.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    let answer = client.ask(ClientMessage::Hello {
        version: "0.0.1-from-another-build".into(),
    });

    match answer {
        ServerMessage::Error { message } => {
            assert!(message.contains("0.0.1-from-another-build"), "{message}");
            assert!(message.contains(&version()), "{message}");
        }
        other => panic!("expected a mismatch report, got {other:?}"),
    }
}

#[test]
fn listing_serves_the_repositories_the_daemon_was_started_with() {
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = Client::attach(daemon.path());

    client.send(ClientMessage::ListRepos);

    assert_eq!(repo_paths(&client.next_repos()), vec![path]);
    drop(repo);
}

#[test]
fn opening_a_repository_adds_it_and_answers_with_the_whole_set() {
    // The whole set, not the one repository: the client renders tabs from it,
    // and another client may have changed it in between.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    client.send(ClientMessage::OpenRepo { path: path.clone() });

    assert_eq!(repo_paths(&client.next_repos()), vec![resolved(&path)]);
    drop(repo);
}

#[test]
fn opening_a_path_that_is_not_a_directory_is_refused_without_closing_the_connection() {
    // A refused request is an answer. The client must still be able to ask the
    // next one.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    let refused = client.ask(ClientMessage::OpenRepo {
        path: "/no/such/place".into(),
    });
    assert!(matches!(refused, ServerMessage::Error { .. }));

    let answer = client.ask(ClientMessage::ListRepos);
    assert!(repo_paths(&answer).is_empty(), "the session is unchanged");
}

#[test]
fn opening_an_empty_path_is_refused() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    let answer = client.ask(ClientMessage::OpenRepo { path: "   ".into() });

    match answer {
        ServerMessage::Error { message } => assert!(message.contains("path"), "{message}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn closing_an_unknown_repository_is_refused() {
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    let answer = client.ask(ClientMessage::CloseRepo {
        repo: "r-nonexistent".into(),
    });

    match answer {
        ServerMessage::Error { message } => assert!(message.contains("unknown"), "{message}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn a_repository_one_client_opens_reaches_another_without_it_asking() {
    // The point of the daemon, and the reason it speaks unprompted: the
    // session is shared, so a change is news for every client, not a reply
    // owed to the one that made it.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut first = Client::attach(daemon.path());
    let mut second = Client::attach(daemon.path());

    first.send(ClientMessage::OpenRepo { path: path.clone() });

    assert_eq!(repo_paths(&second.next_repos()), vec![resolved(&path)]);
    assert_eq!(repo_paths(&first.next_repos()), vec![resolved(&path)]);
    drop(repo);
}

#[test]
fn attaching_serves_the_repository_set_before_anything_is_asked() {
    // A client needs the session's shape to render at all; making it ask for
    // what the daemon already knows is a round trip for nothing.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));

    let mut client = Client::attach_raw(daemon.path());

    assert_eq!(repo_paths(&client.next_repos()), vec![path]);
    drop(repo);
}

#[test]
fn a_refusal_reaches_only_the_client_that_asked() {
    // A bad path is an answer for one client and noise for the rest; a
    // broadcast here would make every attached TUI flash an error.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut asker = Client::attach(daemon.path());
    let mut other = Client::attach(daemon.path());

    let refused = asker.ask(ClientMessage::OpenRepo {
        path: "/no/such/place".into(),
    });
    assert!(matches!(refused, ServerMessage::Error { .. }));

    // The other client has nothing waiting: a short timeout is the only way to
    // assert an absence, and it must not be mistaken for a slow daemon — so a
    // request of its own is answered first, proving the connection is live.
    other.send(ClientMessage::ListRepos);
    assert!(matches!(other.next_repos(), ServerMessage::Repos { .. }));
}

#[test]
fn a_client_that_detaches_leaves_the_session_running_for_the_others() {
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut staying = Client::attach(daemon.path());
    let leaving = Client::attach(daemon.path());

    drop(leaving);

    staying.send(ClientMessage::ListRepos);
    assert_eq!(repo_paths(&staying.next_repos()), vec![path]);
    drop(repo);
}

#[test]
fn an_unreadable_request_is_answered_rather_than_fatal() {
    // Reaching the socket is authorization, not a promise of well-formed input.
    // A client bug must not take the connection down with it.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    write_frame(&mut client.stream, &Frame::control(b"{not json".to_vec())).unwrap();
    client.stream.flush().unwrap();
    let frame = read_frame(&mut client.stream).unwrap().expect("an answer");
    let answer: ServerMessage = serde_json::from_slice(&frame.payload).unwrap();
    assert!(matches!(answer, ServerMessage::Error { .. }));

    client.send(ClientMessage::ListRepos);
    assert!(
        repo_paths(&client.next_repos()).is_empty(),
        "the connection still serves"
    );
}

#[test]
fn a_terminal_frame_arriving_early_is_ignored_not_fatal() {
    // Panes are not shared yet, so there is nothing to write these to. They
    // must not desynchronize the control exchange.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    write_frame(&mut client.stream, &Frame::terminal(vec![1, 2, 3])).unwrap();
    client.stream.flush().unwrap();

    client.send(ClientMessage::ListRepos);
    assert!(repo_paths(&client.next_repos()).is_empty());
}

#[test]
fn attaching_subscribes_to_the_terminals_of_every_open_repository() {
    // Without asking. A client renders a tab per repository, and a pane whose
    // output it never subscribed to would fall behind its own scrollback.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));

    let mut client = Client::attach_raw(daemon.path());

    let (id, event) = client.next_terminal_event();
    assert!(!id.is_empty(), "the event says which repository it is for");
    // A fresh hub offers its startup terminals to be sized before creating
    // them, so this is the first thing any client hears.
    assert!(
        matches!(event, HubServerMessage::Pending { .. }),
        "got {event:?}"
    );
    drop(repo);
}

#[test]
fn a_pane_a_client_creates_streams_its_output_back() {
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = Client::attach(daemon.path());
    let (id, _) = client.next_terminal_event();

    client.send(ClientMessage::Terminal {
        repo: id.clone(),
        message: HubClientMessage::Create { rows: 24, cols: 80 },
    });

    // The shell says something as soon as it starts — a prompt at the very
    // least — and it arrives tagged with the repository it belongs to.
    let output = client.next_output();
    assert_eq!(output.repo, id);
    assert!(!output.data.is_empty());
    drop(repo);
}

#[test]
fn a_pane_one_client_creates_is_streamed_to_another() {
    // The point of sharing the terminals: two clients on one session are
    // looking at the same shell, not one each.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut creator = Client::attach(daemon.path());
    let mut watcher = Client::attach(daemon.path());
    let (id, _) = creator.next_terminal_event();

    creator.send(ClientMessage::Terminal {
        repo: id.clone(),
        message: HubClientMessage::Create { rows: 24, cols: 80 },
    });

    let output = watcher.next_output();
    assert_eq!(output.repo, id, "and the watcher knows which repository");
    assert!(!output.data.is_empty());
    drop(repo);
}

#[test]
fn a_new_pane_names_its_requester_to_that_client_and_nobody_to_the_others() {
    // Every client is told about every pane, so "did I ask for this?" is the
    // only thing that can decide whether it takes the focus — and it has to be
    // answerable with the id from the client's own handshake, since a client
    // never learns its per-repository ids inside the daemon.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut creator = Client::attach(daemon.path());
    let mut watcher = Client::attach(daemon.path());
    let creator_id = creator.hello();
    let watcher_id = watcher.hello();
    let repo_id = creator.repo_ids().pop().expect("one repository is open");

    creator.send(ClientMessage::Terminal {
        repo: repo_id,
        message: HubClientMessage::Create { rows: 24, cols: 80 },
    });

    let (pane, requester) = creator.next_created();
    assert_eq!(requester, Some(creator_id), "the asker's own id");
    let (same_pane, requester) = watcher.next_created();
    assert_eq!(same_pane, pane, "both are told about the one pane");
    assert_eq!(
        requester, None,
        "and the other client is not told it asked (its id is {watcher_id})"
    );
    drop(repo);
}

#[test]
fn a_terminal_request_for_an_unknown_repository_is_dropped_not_fatal() {
    // The client can be a beat behind a close on another one.
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, &[]);
    let mut client = Client::attach(daemon.path());

    client.send(ClientMessage::Terminal {
        repo: "r-nonexistent".into(),
        message: HubClientMessage::Create { rows: 24, cols: 80 },
    });

    client.send(ClientMessage::ListRepos);
    assert!(
        repo_paths(&client.next_repos()).is_empty(),
        "the connection still serves"
    );
}
