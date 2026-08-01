use super::harness::*;
use crate::daemon::frame::{Frame, read_frame, write_frame};
use crate::daemon::protocol::{ClientMessage, ServerMessage, version};
use std::io::Write;

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

    assert_eq!(repo_paths(&client.next_repos()), vec![resolved(&path)]);
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

    assert_eq!(repo_paths(&client.next_repos()), vec![resolved(&path)]);
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
    assert_eq!(repo_paths(&staying.next_repos()), vec![resolved(&path)]);
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
fn asking_for_the_set_answers_the_asker_and_leaves_the_others_alone() {
    // The set is sent from one place now, which is the watcher — but a question
    // is still not news. A client that asked nothing must not be woken by
    // somebody else asking.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut asker = Client::attach(daemon.path());
    let mut quiet = Client::attach(daemon.path());

    asker.send(ClientMessage::ListRepos);

    assert!(matches!(asker.next_repos(), ServerMessage::Repos { .. },));
    quiet
        .stream
        .set_read_timeout(Some(std::time::Duration::from_millis(400)))
        .expect("sets a timeout");
    // Terminal traffic is expected — subscribing a repository with a startup
    // pane offers it to be sized straight away — so this is about the set
    // alone. Reads until the socket goes quiet.
    while let Ok(Some(frame)) = crate::daemon::frame::read_frame(&mut quiet.stream) {
        let heard: Result<ServerMessage, _> = serde_json::from_slice(&frame.payload);
        assert!(
            !matches!(heard, Ok(ServerMessage::Repos { .. })),
            "the other client was sent a set it never asked for"
        );
    }
    drop(repo);
}
