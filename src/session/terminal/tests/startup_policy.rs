use super::{
    attach, created_pane, next_matching, pending_count, spawn_hub, spawn_hub_with_auto_open,
};
use crate::session::terminal::frame::ClientMessage;
use std::time::{Duration, Instant};

#[test]
fn auto_open_offers_one_shell_for_an_empty_startup() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = attach(&hub);

    assert_eq!(
        next_matching(&session, |frame| pending_count(frame).is_some())
            .and_then(|frame| pending_count(&frame)),
        Some(1),
        "auto-open must offer one bare shell"
    );
    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    assert!(
        next_matching(&session, |frame| created_pane(frame).is_some()).is_some(),
        "a client that measured nothing must still get its shell"
    );
    hub.stop();
}

#[test]
fn an_empty_startup_opens_nothing_by_default_until_a_client_creates_a_pane() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub =
        spawn_hub_with_auto_open(&dir.path().to_string_lossy(), Vec::new(), Vec::new(), false);
    let session = attach(&hub);

    let quiet_until = Instant::now() + Duration::from_millis(100);
    while Instant::now() < quiet_until {
        if let Some(frame) = session.next_frame(Duration::from_millis(10)) {
            assert_eq!(pending_count(&frame), None, "must not offer a terminal");
            assert_eq!(created_pane(&frame), None, "must not create a terminal");
        }
    }
    assert!(hub.pane_ids().is_empty());

    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    assert!(
        next_matching(&session, |frame| created_pane(frame).is_some()).is_some(),
        "a later create request must still open the first shell"
    );
    hub.stop();
}
