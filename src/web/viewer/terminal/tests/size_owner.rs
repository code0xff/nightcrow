//! Who decides a pane's size.
//!
//! A PTY is a contract with a child process: it draws for the width it was told,
//! and nothing can re-flow an alternate-screen program afterwards. So the size
//! is one value with one owner, and these are the rules for who holds it.

use super::{attach, next_matching, resized_size, spawn_hub};
use crate::web::viewer::terminal::frame::{ClientMessage, PaneSize, TerminalFrame};
use crate::web::viewer::terminal::{TerminalHub, TerminalSession};

/// Whether a frame says this session owns the sizing.
fn owned(frame: &TerminalFrame) -> Option<bool> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "size_owner" {
        return None;
    }
    value["owned"].as_bool()
}

/// Where this session stands now: the last ownership verdict in its queue.
///
/// The queue is a history, not a state — a client that has been overtaken twice
/// has both verdicts waiting — so a test asking "does it own it" has to read to
/// the end. Ownership changes are queued before the call that causes them
/// returns, so anything pending is already there.
fn verdict(session: &TerminalSession) -> bool {
    let mut last = next_matching(session, |f| owned(f).is_some())
        .and_then(|f| owned(&f))
        .expect("no ownership verdict arrived");
    while let Some(frame) = session.next_frame(QUIET) {
        if let Some(owned) = owned(&frame) {
            last = owned;
        }
    }
    last
}

/// Long enough for a frame the hub has already queued to be read, short enough
/// that a test asserting silence does not sit through the shell deadline.
const QUIET: std::time::Duration = std::time::Duration::from_millis(100);

#[test]
fn the_client_that_just_arrived_owns_the_sizing() {
    // tmux's `window-size latest`: the newest client is the one someone is
    // sitting at, so the panes should fit its screen.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());

    let first = attach(&hub);
    assert!(verdict(&first), "the only client owns it");

    let second = attach(&hub);

    assert!(verdict(&second), "and then the newer one does");
    assert!(!verdict(&first), "which the older one is told");
    hub.stop();
}

#[test]
fn the_sizing_passes_to_the_newest_client_still_attached() {
    // Somebody has to hold it, or the panes stay frozen at the size of a client
    // that has gone — but not at once. A viewer switching repositories closes
    // one socket and opens another, and re-fitting every pane in that gap and
    // back costs an alternate-screen program a repaint each way. So the release
    // waits out `RELEASE_GRACE`, which the hub's own tick is what ends.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let first = attach(&hub);
    let second = attach(&hub);
    let third = attach(&hub);
    assert!(verdict(&third));

    drop(third);

    // Inside the grace nothing has moved: the owner may be a beat from coming
    // back, and a verdict sent here would be one it has to take back.
    assert!(!verdict(&second), "not while the grace is still running");

    let heir = super::wait_for(|| {
        let mut latest = None;
        while let Some(frame) = second.next_frame(QUIET) {
            if let Some(owned) = owned(&frame) {
                latest = Some(owned);
            }
        }
        latest
    });
    assert_eq!(heir, Some(true), "the most recent of those left takes it");
    assert!(!verdict(&first), "not the oldest");
    hub.stop();
}

/// The sizing is the session's, not each repository's.
///
/// Every client shows the same repository — which one is in front is shared — so
/// "which screen are the panes fitted to" has one answer. Asked per hub it was
/// re-answered from scratch on every switch, and with two pages attached the
/// winner was whichever handshake finished last.
#[test]
fn one_answer_covers_every_repository_in_the_session() {
    let dir = tempfile::TempDir::new().unwrap();
    let cwd = dir.path().to_string_lossy().to_string();
    let ownership: std::sync::Arc<crate::web::viewer::size_owner::SizeOwnership> =
        Default::default();
    let one = TerminalHub::spawn(&cwd, Vec::new(), Vec::new(), ownership.clone());
    let two = TerminalHub::spawn(&cwd, Vec::new(), Vec::new(), ownership);

    // An attached terminal subscribes to every open repository at once. Those
    // are one screen, so only its first subscription is an arrival.
    let tui = crate::web::viewer::size_owner::ViewerId::Attached(1);
    let tui_one = one.connect(tui.clone(), true);
    let tui_two = two.connect(tui, false);
    assert!(verdict(&tui_one));
    assert!(verdict(&tui_two), "both of its ends hold the sizing");

    // A page opens on the second repository and takes it, as `window-size
    // latest` says.
    let page = attach(&two);

    assert!(verdict(&page));
    assert!(
        !verdict(&tui_one),
        "the displaced viewer stops sizing everywhere, not just where it lost"
    );
    assert!(!verdict(&tui_two));
    one.stop();
    two.stop();
}

#[test]
fn a_client_can_take_the_sizing_back_on_request() {
    // The explicit half of the policy: looking at a pane does not move the
    // sizing, asking for it does. Otherwise glancing at a phone would repaint
    // everybody's screen.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let first = attach(&hub);
    let second = attach(&hub);
    assert!(verdict(&second));
    assert!(!verdict(&first));

    first.dispatch(ClientMessage::ClaimSize);

    assert!(verdict(&first), "the asker has it");
    assert!(!verdict(&second), "and the previous owner is told");
    hub.stop();
}

#[test]
fn claiming_what_this_client_already_owns_says_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let only = attach(&hub);
    assert!(verdict(&only));

    only.dispatch(ClientMessage::ClaimSize);

    // Nothing to tell anyone: a repeated claim is not a change.
    assert!(
        only.next_frame(QUIET).as_ref().and_then(owned).is_none(),
        "a no-op claim must not announce anything"
    );
    hub.stop();
}

#[test]
fn only_the_owner_resizes_the_pty_and_everyone_is_told_the_size() {
    // Two clients fitting one PTY to two layouts would leave the child drawing
    // for a width neither of them has.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let first = attach(&hub);
    first.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 24, cols: 80 }],
    });
    let pane = super::collect_created(&first, 1)[0];
    // The newcomer takes the sizing from `first`.
    let second = attach(&hub);
    assert!(verdict(&second));

    // Both ask, in this order, through the one command queue the hub drains.
    first.dispatch(ClientMessage::Resize {
        pane,
        rows: 40,
        cols: 120,
    });
    second.dispatch(ClientMessage::Resize {
        pane,
        rows: 30,
        cols: 100,
    });

    // So the first `resized` to come back is the owner's — the other request
    // reached the hub first and was dropped.
    let applied = next_matching(&second, |f| resized_size(f).is_some())
        .and_then(|f| resized_size(&f))
        .expect("the owner's resize was not applied");
    assert_eq!(applied, (30, 100));
    // And the client that no longer owns the sizing is told what the size is,
    // because its emulator has to wrap where the child now does.
    let told = next_matching(&first, |f| resized_size(f).is_some())
        .and_then(|f| resized_size(&f))
        .expect("a spectator was not told the new size");
    assert_eq!(told, (30, 100));
    hub.stop();
}
