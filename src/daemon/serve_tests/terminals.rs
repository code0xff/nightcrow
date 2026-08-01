use super::harness::*;
use crate::daemon::frame::{Frame, write_frame};
use crate::daemon::protocol::ClientMessage;
use crate::session::terminal::frame::{
    ClientMessage as HubClientMessage, ServerMessage as HubServerMessage,
};
use std::io::Write;

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

    let (id, _) = client.next_terminal_event();
    assert!(!id.is_empty(), "the event says which repository it is for");
    // And the subscription is live from the start: a fresh hub offers its
    // startup terminals to be sized before creating them, so that offer reaches
    // a client that has asked for nothing.
    let mut offered = false;
    for _ in 0..8 {
        let (_, event) = client.next_terminal_event();
        if matches!(event, HubServerMessage::Pending { .. }) {
            offered = true;
            break;
        }
    }
    assert!(offered, "the startup terminals were never offered");
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
