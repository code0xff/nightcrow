use super::*;
use crate::daemon::frame::{FrameKind, read_frame};
use crate::daemon::protocol::ClientMessage;
use crate::daemon::terminal_link::TerminalRouter;
use crate::daemon::transport::UnixStream;
use std::sync::{Arc, Mutex};

const REPO: &str = "r1";
const MINE: u64 = 7;

/// A backend on a socket pair, with the router the daemon side would fill and
/// the far end to read requests off.
struct Wired {
    backend: HubBackend,
    router: Arc<TerminalRouter>,
    daemon: UnixStream,
}

fn wired() -> Wired {
    let (client, daemon) = UnixStream::pair().expect("a socket pair");
    let router = Arc::new(TerminalRouter::default());
    let link = TerminalLink::new(
        REPO,
        Arc::new(Mutex::new(client)),
        Arc::clone(&router),
        MINE,
    );
    Wired {
        backend: HubBackend::new(link),
        router,
        daemon,
    }
}

impl Wired {
    /// The next request the daemon would see.
    fn next_request(&mut self) -> ClientMessage {
        let frame = read_frame(&mut self.daemon)
            .expect("reads")
            .expect("the client speaks");
        assert_eq!(frame.kind, FrameKind::Control);
        serde_json::from_slice(&frame.payload).expect("decodes")
    }

    fn deliver(&self, event: HubServerMessage) {
        self.router.deliver(REPO, TerminalMessage::Event(event));
    }
}

#[test]
fn a_create_is_a_request_tagged_with_its_repository() {
    // One socket carries every repository, so a request that did not name one
    // would open the pane in whichever the daemon guessed.
    let mut wired = wired();

    wired.backend.create_pane(24, 80, None).expect("asks");

    match wired.next_request() {
        ClientMessage::Terminal { repo, message } => {
            assert_eq!(repo, REPO);
            assert!(matches!(
                message,
                HubClientMessage::Create { rows: 24, cols: 80 }
            ));
        }
        other => panic!("expected a terminal request, got {other:?}"),
    }
}

#[test]
fn a_pane_running_a_command_is_refused_rather_than_opened_bare() {
    // The session's configured commands are the daemon's to run, once. Silently
    // opening a bare shell instead would look like the command had run.
    let mut wired = wired();

    let refused = wired.backend.create_pane(24, 80, Some("claude"));

    assert!(refused.is_err());
    assert!(format!("{}", refused.unwrap_err()).contains("claude"));
}

#[test]
fn input_reaches_the_pane_as_the_bytes_that_were_typed() {
    let mut wired = wired();

    wired.backend.send_input(3, b"\x1b[A").expect("sends");

    match wired.next_request() {
        ClientMessage::Terminal {
            message: HubClientMessage::Input { pane, data },
            ..
        } => {
            assert_eq!(pane, 3);
            assert_eq!(data.as_bytes(), b"\x1b[A");
        }
        other => panic!("expected input, got {other:?}"),
    }
}

#[test]
fn input_that_is_not_valid_utf8_is_reported_rather_than_mangled() {
    // Everything a client sends is UTF-8 by construction, so this is a bug on
    // this side — and lossy encoding would hand the shell different bytes than
    // the ones it was given.
    let mut wired = wired();

    assert!(wired.backend.send_input(1, &[0xff, 0xfe]).is_err());
}

#[test]
fn a_pane_this_client_asked_for_is_reported_as_its_own() {
    let mut wired = wired();
    wired.deliver(HubServerMessage::Created {
        pane: 1,
        rows: 24,
        cols: 80,
        client: Some(MINE),
        title: None,
    });

    let events = wired.backend.drain_events();

    assert!(matches!(
        events.as_slice(),
        [BackendEvent::Created {
            pane: 1,
            requested: true,
            ..
        }]
    ));
}

#[test]
fn a_pane_another_client_opened_arrives_without_claiming_the_focus() {
    // Which pane this client is looking at is its own business; a terminal
    // someone else opened in the browser must not move it.
    let mut wired = wired();
    wired.deliver(HubServerMessage::Created {
        pane: 2,
        rows: 24,
        cols: 80,
        client: Some(MINE + 1),
        title: None,
    });
    wired.deliver(HubServerMessage::Created {
        pane: 3,
        rows: 24,
        cols: 80,
        client: None,
        title: None,
    });

    let events = wired.backend.drain_events();

    assert!(matches!(
        events.as_slice(),
        [
            BackendEvent::Created {
                requested: false,
                ..
            },
            BackendEvent::Created {
                requested: false,
                ..
            }
        ]
    ));
}

#[test]
fn output_and_exits_come_through_as_they_are() {
    let mut wired = wired();
    wired.router.deliver(
        REPO,
        TerminalMessage::Output {
            pane: 1,
            // Not valid UTF-8: a multi-byte sequence split across reads is
            // routine, and the emulator is what reassembles it.
            data: vec![0xe2, 0x94],
        },
    );
    wired.deliver(HubServerMessage::Exited { pane: 1 });

    let events = wired.backend.drain_events();

    assert!(matches!(
        events.as_slice(),
        [
            BackendEvent::Output { pane: 1, .. },
            BackendEvent::Exited { pane: 1 }
        ]
    ));
    match &events[0] {
        BackendEvent::Output { data, .. } => assert_eq!(data, &vec![0xe2, 0x94]),
        other => panic!("expected output, got {other:?}"),
    }
}

#[test]
fn the_startup_terminals_are_answered_with_no_sizes_and_no_event() {
    // Nothing has been measured when the offer arrives — it comes on attach,
    // before the first frame — so the hub's default opens them and the first
    // layout corrects it. Answering with a made-up size would be the same
    // repaint plus a wrong number.
    let mut wired = wired();
    wired.deliver(HubServerMessage::Pending { count: 2 });

    let events = wired.backend.drain_events();

    assert!(events.is_empty(), "the offer is not a pane");
    match wired.next_request() {
        ClientMessage::Terminal {
            message: HubClientMessage::Start { sizes },
            ..
        } => assert!(sizes.is_empty()),
        other => panic!("expected a start, got {other:?}"),
    }
}

#[test]
fn claiming_the_sizing_asks_the_session_rather_than_assuming_it() {
    // The answer comes back as the session granting it, which is also what
    // re-fits the panes. Assuming it here would leave a client resizing panes it
    // does not own.
    let mut wired = wired();

    wired.backend.claim_size();

    assert!(matches!(
        wired.next_request(),
        ClientMessage::Terminal {
            message: HubClientMessage::ClaimSize,
            ..
        }
    ));
    assert!(
        wired.backend.drain_events().is_empty(),
        "nothing is granted locally"
    );
}

#[test]
fn a_reorder_is_asked_for_and_the_order_comes_back_as_an_event() {
    // The order is the session's, so this end only requests it — and hears the
    // result the same way it would hear about a reorder in the browser.
    let mut wired = wired();

    wired.backend.reorder(&[3, 1, 2]);

    match wired.next_request() {
        ClientMessage::Terminal {
            message: HubClientMessage::Reorder { order },
            ..
        } => assert_eq!(order, vec![3, 1, 2]),
        other => panic!("expected a reorder, got {other:?}"),
    }
    assert!(
        wired.backend.drain_events().is_empty(),
        "nothing is applied locally"
    );

    wired.deliver(HubServerMessage::Reordered {
        order: vec![3, 1, 2],
    });
    assert!(matches!(
        wired.backend.drain_events().as_slice(),
        [BackendEvent::Reordered { order }] if order == &[3, 1, 2]
    ));
}
