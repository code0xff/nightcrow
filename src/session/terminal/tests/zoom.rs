//! Which pane fills the panel: the hub's answer, broadcast and replayed.

use super::{attach, collect_created, created_pane, next_matching, spawn_hub, zoomed_pane};
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{ClientMessage, PaneSize};

/// The zoom `session` is told about next.
fn next_zoom(session: &TerminalSession) -> Option<Option<crate::backend::PaneId>> {
    next_matching(session, |f| zoomed_pane(f).is_some()).and_then(|f| zoomed_pane(&f))
}

/// Open `n` panes: the startup shell (claimed with a size, as a client does)
/// and any others by request, returned in creation order.
fn open_panes(session: &TerminalSession, n: usize) -> Vec<crate::backend::PaneId> {
    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 24, cols: 80 }],
    });
    for _ in 1..n {
        session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    }
    collect_created(session, n)
}

#[test]
fn zooming_a_pane_echoes_it_and_replays_it_to_a_later_joiner() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let first = attach(&hub);
    let ids = open_panes(&first, 2);

    first.dispatch(ClientMessage::Zoom { pane: Some(ids[0]) });

    assert_eq!(
        next_zoom(&first),
        Some(Some(ids[0])),
        "the hub must echo the pane it zoomed to the client that asked"
    );

    // A page that reloads is a new connection to the same hub: it must come back
    // to the zoom rather than to the grid. This is the whole point — a zoom used
    // to live in one page's state and was lost with it.
    //
    // Ahead of the panes, and that order is the contract: a client that learned
    // the zoom after replaying their histories could settle its layout on the
    // grid first and resize every PTY twice.
    let second = attach(&hub);
    let first = next_matching(&second, |f| {
        zoomed_pane(f).is_some() || created_pane(f).is_some()
    })
    .expect("a connecting client was told neither the panes nor the zoom");
    assert_eq!(
        zoomed_pane(&first),
        Some(Some(ids[0])),
        "the zoom must survive a fresh connection, and arrive before the panes"
    );
    hub.stop();
}

#[test]
fn un_zooming_is_announced_rather_than_left_to_be_inferred() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = attach(&hub);
    let ids = open_panes(&session, 1);

    session.dispatch(ClientMessage::Zoom { pane: Some(ids[0]) });
    assert_eq!(next_zoom(&session), Some(Some(ids[0])));

    session.dispatch(ClientMessage::Zoom { pane: None });

    assert_eq!(
        next_zoom(&session),
        Some(None),
        "every client has to be told the panel went back to the grid"
    );
    hub.stop();
}

#[test]
fn zooming_a_pane_that_does_not_exist_is_ignored() {
    // A client racing a pane exit is normal traffic. The zoom must not take, or
    // every client renders a panel filled with a pane that is not there.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = attach(&hub);
    let ids = open_panes(&session, 1);

    session.dispatch(ClientMessage::Zoom { pane: Some(9999) });
    // Asked for right after, so the first `zoomed` frame to arrive settles both
    // questions: the unknown pane was dropped, and the hub is still serving.
    session.dispatch(ClientMessage::Zoom { pane: Some(ids[0]) });

    assert_eq!(
        next_zoom(&session),
        Some(Some(ids[0])),
        "an unknown pane must never become the zoomed one"
    );
    hub.stop();
}

#[test]
fn opening_a_pane_ends_the_zoom_before_announcing_it() {
    // Otherwise the terminal somebody just asked for opens behind a pane filling
    // the panel, and nothing on screen says it is there.
    //
    // The order is the contract, not merely that both frames arrive: a client
    // renders between the two, and one told about the pane while still zoomed
    // past it hides the new terminal for that render — and points the keyboard
    // at the pane filling the panel rather than the one it just opened.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = attach(&hub);
    let ids = open_panes(&session, 1);

    session.dispatch(ClientMessage::Zoom { pane: Some(ids[0]) });
    assert_eq!(next_zoom(&session), Some(Some(ids[0])));

    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

    let first = next_matching(&session, |f| {
        zoomed_pane(f).is_some() || created_pane(f).is_some()
    })
    .expect("neither the un-zoom nor the new pane arrived");
    assert_eq!(
        zoomed_pane(&first),
        Some(None),
        "the zoom must end before the pane that ends it is announced"
    );
    hub.stop();
}

#[test]
fn closing_the_zoomed_pane_ends_the_zoom() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = attach(&hub);
    let ids = open_panes(&session, 2);

    session.dispatch(ClientMessage::Zoom { pane: Some(ids[1]) });
    assert_eq!(next_zoom(&session), Some(Some(ids[1])));

    session.dispatch(ClientMessage::Close { pane: ids[1] });

    assert_eq!(
        next_zoom(&session),
        Some(None),
        "a zoom must not outlive the pane it names"
    );
    hub.stop();
}
