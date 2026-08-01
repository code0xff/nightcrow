//! Telling one client's own pane from another's.

use super::{attach, created_pane, created_requester, hello_client, next_matching, spawn_hub};
use crate::web::viewer::terminal::frame::ClientMessage;

#[test]
fn a_connecting_client_is_told_its_own_id_first() {
    // First, because everything that follows can carry a requester id and the
    // client has to be able to judge it.
    //
    // The hub is given a pane before this client arrives, so the replay puts a
    // `created` on the queue too and the assertion is about *order* rather than
    // about `hello` merely turning up: registering the client ahead of the hello
    // would put a broadcast in front of it and this must fail.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let existing = attach(&hub);
    existing.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    next_matching(&existing, |f| created_pane(f).is_some()).expect("no created message");

    let session = attach(&hub);
    let first = next_matching(&session, |f| {
        hello_client(f).is_some() || created_pane(f).is_some()
    })
    .expect("neither a hello nor the replayed pane arrived");
    assert_eq!(
        hello_client(&first),
        Some(session.client_id()),
        "a client must be told its own id before anything that names one"
    );
    hub.stop();
}

#[test]
fn a_new_pane_names_the_client_that_asked_and_nobody_else() {
    // The race this exists for: two clients create at once, and each has to
    // credit itself with its own pane rather than whichever came back first.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let asking = attach(&hub);
    let watching = attach(&hub);

    asking.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

    let mine = next_matching(&asking, |f| created_pane(f).is_some()).expect("no created message");
    assert_eq!(
        created_requester(&mine),
        Some(Some(asking.client_id())),
        "the pane must name the client that asked for it"
    );

    // The same pane, seen by the client that did not ask: it carries the other
    // client's id, which is exactly what makes the comparison worth doing.
    let theirs =
        next_matching(&watching, |f| created_pane(f).is_some()).expect("no created message");
    assert_eq!(created_pane(&theirs), created_pane(&mine));
    assert_ne!(
        created_requester(&theirs),
        Some(Some(watching.client_id())),
        "a client must not be able to read another's pane as its own"
    );
    hub.stop();
}

#[test]
fn a_replayed_pane_names_no_requester() {
    // What a reconnecting page must not focus: panes that were already there.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let first = attach(&hub);
    first.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    next_matching(&first, |f| created_pane(f).is_some()).expect("no created message");

    let second = attach(&hub);
    let replayed = next_matching(&second, |f| created_pane(f).is_some()).expect("no replayed pane");
    assert_eq!(
        created_requester(&replayed),
        Some(None),
        "a replayed pane belongs to nobody on this connection"
    );
    hub.stop();
}
