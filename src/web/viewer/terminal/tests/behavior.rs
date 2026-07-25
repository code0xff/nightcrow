use super::{collect_created, created_pane, next_matching, reordered_order};
use crate::backend::PaneId;
use crate::web::viewer::limits;
use crate::web::viewer::terminal::TerminalHub;
use crate::web::viewer::terminal::frame::{ClientMessage, TerminalFrame};

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

    // The startup shell plus one explicit create give two panes to reorder,
    // captured in their creation (== current) order.
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

#[test]
fn the_first_connection_spawns_a_startup_terminal() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(
        &dir.path().to_string_lossy(),
        vec!["printf hello".to_string()],
    );
    // Connecting is enough — no client Create is dispatched — to launch the
    // configured startup terminal.
    let session = hub.connect();
    let created = next_matching(
        &session,
        |f| matches!(f, TerminalFrame::Control(json) if json.contains("created")),
    );
    assert!(
        created.is_some(),
        "the startup terminal was not spawned on connect"
    );
    hub.stop();
}

#[test]
fn an_empty_startup_opens_one_shell_on_the_first_connection() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let session = hub.connect();
    let created = next_matching(
        &session,
        |f| matches!(f, TerminalFrame::Control(json) if json.contains("created")),
    );
    assert!(
        created.is_some(),
        "a default shell should open on the first connect"
    );
    hub.stop();
}
