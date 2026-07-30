use super::{
    collect_created, created_pane, created_size, next_matching, reordered_order, resized_size,
};
use crate::backend::PaneId;
use crate::web::viewer::limits;
use crate::web::viewer::terminal::TerminalHub;
use crate::web::viewer::terminal::frame::{ClientMessage, PaneSize, TerminalFrame};

#[test]
fn creating_a_terminal_announces_it_and_streams_output() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let session = hub.connect();

    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

    // A create is announced synchronously under the state lock, so this
    // arrives as soon as the worker services the command.
    let created =
        next_matching(&session, |f| created_pane(f).is_some()).expect("no created message");
    let pane = created_pane(&created).unwrap();

    // Drive deterministic output instead of waiting on the interactive
    // prompt, whose timing depends on the user's rc chain and was flaky
    // under parallel load. The marker proves the stream is live end to end.
    let marker = "nightcrow-live";
    session.dispatch(ClientMessage::Input {
        pane,
        data: format!("printf {marker}\n"),
    });

    let output = next_matching(&session, |f| {
        matches!(f, TerminalFrame::Output { pane: p, data }
            if *p == pane && String::from_utf8_lossy(data).contains(marker))
    });
    assert!(output.is_some(), "no output from the shell");
    hub.stop();
}

#[test]
fn the_per_repo_terminal_cap_is_enforced() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let session = hub.connect();

    for _ in 0..limits::MAX_PTYS_PER_REPO + 2 {
        session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    }

    let refused = next_matching(
        &session,
        |f| matches!(f, TerminalFrame::Control(json) if json.contains("terminal limit reached")),
    );
    assert!(
        refused.is_some(),
        "the cap must refuse the extra terminals, not spawn them"
    );
    hub.stop();
}

#[test]
fn a_dropped_session_stops_receiving() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());

    let session = hub.connect();
    assert_eq!(hub.client_count(), 1);
    drop(session);

    assert_eq!(hub.client_count(), 0);
    hub.stop();
}

#[test]
fn reordering_panes_echoes_the_order_and_replays_it_to_a_later_joiner() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let first = hub.connect();

    // The startup shell (claimed with a size, as a client does) plus one
    // explicit create give two panes to reorder, captured in their creation
    // (== current) order.
    first.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 24, cols: 80 }],
    });
    first.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let ids = collect_created(&first, 2);
    let reversed: Vec<PaneId> = ids.iter().copied().rev().collect();

    first.dispatch(ClientMessage::Reorder {
        order: reversed.clone(),
    });

    // The sender is told the canonical order that was applied.
    let echoed = next_matching(&first, |f| reordered_order(f).is_some())
        .and_then(|f| reordered_order(&f))
        .expect("no reordered echo");
    assert_eq!(echoed, reversed, "the hub must echo the applied order");

    // A client that connects afterwards replays the panes in the new order,
    // proving the order lives on the server and survives a fresh connection.
    let second = hub.connect();
    assert_eq!(
        collect_created(&second, 2),
        reversed,
        "replay order must follow the reorder"
    );
    hub.stop();
}

#[test]
fn a_reconnecting_client_receives_existing_panes_and_scrollback() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let first = hub.connect();

    first.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let created = next_matching(&first, |f| created_pane(f).is_some()).expect("no created message");
    let pane = created_pane(&created).unwrap();
    // The shell writes a prompt; that is the scrollback a late joiner must
    // get back.
    assert!(
        next_matching(&first, |f| matches!(f, TerminalFrame::Output { .. })).is_some(),
        "no output from the shell"
    );

    // A client that connects afterwards (a refreshed browser) must be told
    // about the pane that already exists and handed its scrollback.
    let second = hub.connect();
    let replayed = next_matching(&second, |f| created_pane(f).is_some())
        .expect("reconnecting client was not told about the existing pane");
    assert_eq!(
        created_pane(&replayed),
        Some(pane),
        "replayed pane id must match the live pane"
    );
    let replay_output = next_matching(
        &second,
        |f| matches!(f, TerminalFrame::Output { pane: p, .. } if *p == pane),
    );
    assert!(
        replay_output.is_some(),
        "reconnecting client did not receive the scrollback"
    );
    hub.stop();
}

#[test]
fn a_replayed_pane_reports_the_size_it_was_last_resized_to() {
    // What a reconnecting page uses to decide it has nothing to resize. If
    // this reported the birth size instead, every reload would send a resize
    // the PTY does not need and cost the child a full repaint.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let first = hub.connect();

    first.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let created = next_matching(&first, |f| created_pane(f).is_some()).expect("no created message");
    let pane = created_pane(&created).unwrap();
    assert_eq!(
        created_size(&created),
        Some((24, 80)),
        "a new pane reports the size it was created with"
    );

    first.dispatch(ClientMessage::Resize {
        pane,
        rows: 40,
        cols: 120,
    });
    // Wait for the resize to be *applied* before connecting, and wait for it on
    // the client that asked — the `resized` broadcast is the worker saying it
    // has landed.
    //
    // Retrying the connection instead would destroy what it waits for.
    // Connecting takes the sizing (`window-size latest`), and a resize from a
    // client that no longer owns it is dropped rather than queued
    // (`hub_run.rs::apply_resize`). So a `connect` that beats the worker to this
    // still-pending resize discards it for good, and no amount of retrying
    // brings the size the test is waiting for — it just spends the whole
    // deadline. Normally the worker wins that race, which is what made the
    // failure rare and load-dependent rather than constant.
    let applied =
        next_matching(&first, |f| resized_size(f).is_some()).and_then(|f| resized_size(&f));
    assert_eq!(
        applied,
        Some((40, 120)),
        "the owner is told the size its resize was applied at"
    );

    let session = hub.connect();
    let replayed =
        next_matching(&session, |f| created_pane(f) == Some(pane)).and_then(|f| created_size(&f));

    assert_eq!(
        replayed,
        Some((40, 120)),
        "a replayed pane must report its current size, not its birth size"
    );
    hub.stop();
}

#[test]
fn input_for_an_unknown_pane_is_ignored() {
    // A client racing a pane exit is normal traffic, not an error worth
    // tearing the connection down for.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let session = hub.connect();

    session.dispatch(ClientMessage::Input {
        pane: 9999,
        data: "rm -rf /\n".to_string(),
    });
    session.dispatch(ClientMessage::Resize {
        pane: 9999,
        rows: 10,
        cols: 10,
    });
    session.dispatch(ClientMessage::Close { pane: 9999 });

    // The hub must still be serving after all three.
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    let created = next_matching(
        &session,
        |f| matches!(f, TerminalFrame::Control(json) if json.contains("created")),
    );
    assert!(created.is_some(), "the hub stopped serving");
    hub.stop();
}

#[test]
fn stop_is_idempotent() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    hub.stop();
    hub.stop();
}
