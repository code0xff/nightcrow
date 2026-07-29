//! The colour the session is painted in.
//!
//! Shared like the project in front, and for the same reason: a session reached
//! from a terminal and a browser at once was showing two colours, with no value
//! able to say which was its own.

use super::harness::*;
use crate::daemon::protocol::ClientMessage;

#[test]
fn setting_the_accent_reaches_the_other_clients() {
    // The point of sharing it. Without this a client only ever sees the colour
    // it picked itself, which is the state this replaced.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut picker = Client::attach(daemon.path());
    let mut watcher = Client::attach(daemon.path());

    picker.send(ClientMessage::SetAccent { accent: 3 });

    assert_eq!(watcher.next_accent(), 3, "the other client follows");
    drop(repo);
}

#[test]
fn the_accent_comes_with_the_first_set_a_client_is_given() {
    // A client that has just attached has not asked for anything yet, and must
    // not have to: it renders before its first request would be answered.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut picker = Client::attach(daemon.path());
    picker.send(ClientMessage::SetAccent { accent: 2 });
    assert_eq!(picker.next_accent(), 2);

    let mut arriving = Client::attach_raw(daemon.path());

    assert_eq!(arriving.next_accent(), 2);
    drop(repo);
}

#[test]
fn an_accent_past_the_end_of_the_cycle_wraps_rather_than_being_refused() {
    // The index is input — a client that drifts out of range self-corrects from
    // what it reads back, rather than being told no and staying wrong.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = Client::attach(daemon.path());

    client.send(ClientMessage::SetAccent {
        accent: crate::config::Accent::ALL.len() + 2,
    });

    assert_eq!(client.next_accent(), 2);
    drop(repo);
}

#[test]
fn an_accent_that_changes_nothing_is_not_announced_again() {
    // The watcher broadcasts on difference, so re-picking the current colour
    // must be silent: a client that redrew on every repeat would flicker under
    // a held key.
    let (repo, path) = crate::test_util::make_repo();
    let dir = tempfile::TempDir::new().unwrap();
    let daemon = daemon(&dir, std::slice::from_ref(&path));
    let mut client = Client::attach(daemon.path());
    client.send(ClientMessage::SetAccent { accent: 4 });
    assert_eq!(client.next_accent(), 4);

    client.send(ClientMessage::SetAccent { accent: 4 });
    // Followed by a real change, so the repeat has something to be in front of:
    // had it been announced, this next frame would read 4 rather than 1.
    client.send(ClientMessage::SetAccent { accent: 1 });

    assert_eq!(client.next_accent(), 1);
    drop(repo);
}
