use super::{
    collect_created, created_pane, created_size, created_title, next_matching, pending_count,
    reordered_order, wait_for,
};
use crate::backend::PaneId;
use crate::config::StartupCommand;
use crate::web::viewer::limits;
use crate::web::viewer::terminal::TerminalHub;
use crate::web::viewer::terminal::frame::{ClientMessage, PaneSize, TerminalFrame};

/// A startup terminal configured with no name of its own, which is what most of
/// these tests care about — they assert on panes, not on what they are called.
fn startup(command: &str) -> StartupCommand {
    StartupCommand {
        name: None,
        command: command.to_string(),
    }
}

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
    // The resize is applied by the worker thread, so wait for the size to
    // reach a fresh connection rather than assuming it has landed.
    let replayed = wait_for(|| {
        let session = hub.connect();
        next_matching(&session, |f| created_pane(f) == Some(pane))
            .and_then(|f| created_size(&f))
            .filter(|size| *size == (40, 120))
    });

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

#[test]
fn a_startup_terminal_is_offered_for_sizing_and_born_at_that_size() {
    // The whole point of the handshake: the child must never draw a frame at a
    // size no client chose, so the PTY does not exist until one has measured.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), vec![startup("printf hello")]);
    let session = hub.connect();

    assert_eq!(
        next_matching(&session, |f| pending_count(f).is_some()).and_then(|f| pending_count(&f)),
        Some(1),
        "connecting must offer the startup terminal rather than spawn it"
    );

    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize {
            rows: 40,
            cols: 120,
        }],
    });

    let created =
        next_matching(&session, |f| created_pane(f).is_some()).expect("no startup terminal");
    assert_eq!(
        created_size(&created),
        Some((40, 120)),
        "the startup terminal must be born at the size the client measured"
    );
    hub.stop();
}

#[test]
fn an_empty_startup_offers_one_shell() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let session = hub.connect();

    assert_eq!(
        next_matching(&session, |f| pending_count(f).is_some()).and_then(|f| pending_count(&f)),
        Some(1),
        "no configured commands still means one bare shell"
    );

    session.dispatch(ClientMessage::Start { sizes: Vec::new() });

    assert!(
        next_matching(&session, |f| created_pane(f).is_some()).is_some(),
        "a client that measured nothing must still get its shell"
    );
    hub.stop();
}

#[test]
fn a_startup_size_of_zero_is_clamped_rather_than_reaching_openpty() {
    // A zero dimension can fail `openpty` outright, and the claim is already
    // spent by then — the hub would hold `started` with no terminal to show
    // for it and never offer them again.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let session = hub.connect();
    next_matching(&session, |f| pending_count(f).is_some()).expect("no offer");

    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 0, cols: 0 }],
    });

    let created = next_matching(&session, |f| created_pane(f).is_some())
        .expect("a zero size must still produce a terminal");
    assert_eq!(
        created_size(&created),
        Some((limits::MIN_PANE_DIMENSION, limits::MIN_PANE_DIMENSION)),
        "the size must be clamped, not passed through"
    );
    hub.stop();
}

#[test]
fn a_startup_set_that_fills_the_cap_still_gets_every_terminal() {
    // The claim reserves the free slots, so a create racing it loses them
    // rather than taking them: the configured set comes up whole and the
    // create is the one refused. Dispatching from one session cannot place a
    // create between the claim and the batch reaching the queue — that window
    // belongs to another connection's handler thread and has no seam to drive
    // it from here — so what this pins is the outcome, not that interleaving.
    let dir = tempfile::TempDir::new().unwrap();
    let configured: Vec<StartupCommand> = (0..limits::MAX_PTYS_PER_REPO)
        .map(|i| startup(&format!("printf startup{i}")))
        .collect();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), configured);
    let session = hub.connect();
    assert_eq!(
        next_matching(&session, |f| pending_count(f).is_some()).and_then(|f| pending_count(&f)),
        Some(limits::MAX_PTYS_PER_REPO),
    );

    session.dispatch(ClientMessage::Start { sizes: Vec::new() });
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

    let ids = collect_created(&session, limits::MAX_PTYS_PER_REPO);
    assert_eq!(ids.len(), limits::MAX_PTYS_PER_REPO);
    assert!(
        next_matching(
            &session,
            |f| matches!(f, TerminalFrame::Control(json) if json.contains("terminal limit reached"))
        )
        .is_some(),
        "the client create should be refused, not a startup command"
    );
    hub.stop();
}

#[test]
fn a_startup_command_the_cap_turned_away_is_named() {
    // Reachable only this way: the configured set can never exceed the cap on
    // its own (config refuses more than `MAX_STARTUP_COMMANDS` entries, which
    // equals the cap), so a command is turned away only when a terminal was
    // already open when the claim happened. The set is spent by then, so it
    // will not run until the hub restarts — the user has to open it by hand,
    // and cannot without being told which one it was.
    let dir = tempfile::TempDir::new().unwrap();
    let configured: Vec<StartupCommand> = (0..limits::MAX_PTYS_PER_REPO)
        .map(|i| startup(&format!("printf startup{i}")))
        .collect();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), configured);
    let session = hub.connect();
    next_matching(&session, |f| pending_count(f).is_some()).expect("no offer");

    // One terminal by hand *first*, and confirmed created — the claim must see
    // it, or it would simply reserve every slot and refuse this instead.
    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });
    next_matching(&session, |f| created_pane(f).is_some()).expect("no manual terminal");

    session.dispatch(ClientMessage::Start { sizes: Vec::new() });

    let refused = next_matching(
        &session,
        |f| matches!(f, TerminalFrame::Control(json) if json.contains("terminal limit reached")),
    )
    .expect("the command with no slot left should be refused");
    let TerminalFrame::Control(json) = refused else {
        unreachable!()
    };
    assert!(
        json.contains(&format!("printf startup{}", limits::MAX_PTYS_PER_REPO - 1)),
        "the message must name the command that did not start: {json}"
    );
    hub.stop();
}

#[test]
fn an_unanswered_offer_is_made_again_to_the_next_client() {
    // A page that closes mid-handshake must not take the terminals with it.
    // Nothing consumes the offer but an answer, so the hub cannot end up with
    // no terminals and no way to ever open them.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());

    let abandoned = hub.connect();
    assert!(
        next_matching(&abandoned, |f| pending_count(f).is_some()).is_some(),
        "the first client was not offered the startup terminals"
    );
    drop(abandoned);

    let second = hub.connect();
    assert_eq!(
        next_matching(&second, |f| pending_count(f).is_some()).and_then(|f| pending_count(&f)),
        Some(1),
        "an offer nobody answered must be made again"
    );
    hub.stop();
}

#[test]
fn only_the_first_answer_opens_the_startup_terminals() {
    // Both clients were offered the panes, so both may answer. Creating them
    // twice would double every configured command.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let first = hub.connect();
    let second = hub.connect();

    first.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 30, cols: 90 }],
    });
    let created =
        next_matching(&first, |f| created_pane(f).is_some()).expect("no startup terminal");
    second.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 10, cols: 10 }],
    });

    // The second answer must not add a pane, and must not resize the first.
    assert!(
        next_matching(&second, |f| created_pane(f).is_some()
            && created_pane(f) != created_pane(&created))
        .is_none(),
        "a second answer must not open another terminal"
    );
    hub.stop();
}

#[test]
fn a_configured_startup_terminal_is_announced_under_its_name() {
    // The session opens these, so the session is what knows what they are
    // called — every client shows the same name, and none of them has to read
    // the config to find it. Without this a `[[startup_command]] name` reached
    // nobody and every startup pane was "shell 1".
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(
        &dir.path().to_string_lossy(),
        vec![StartupCommand {
            name: Some("Claude".into()),
            command: "printf hello".into(),
        }],
    );
    let session = hub.connect();
    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 24, cols: 80 }],
    });

    let created = next_matching(&session, |f| created_pane(f).is_some()).expect("no pane");
    assert_eq!(created_title(&created).as_deref(), Some("Claude"));

    // And a client that connects later is told it too, rather than showing
    // something the other clients do not call it.
    let late = hub.connect();
    let replayed = next_matching(&late, |f| created_pane(f).is_some()).expect("no replay");
    assert_eq!(created_title(&replayed).as_deref(), Some("Claude"));
    hub.stop();
}

#[test]
fn an_unnamed_startup_terminal_falls_back_to_its_command() {
    // What the operator wrote is what they would recognise it by.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), vec![startup("printf hello")]);
    let session = hub.connect();
    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 24, cols: 80 }],
    });

    let created = next_matching(&session, |f| created_pane(f).is_some()).expect("no pane");
    assert_eq!(created_title(&created).as_deref(), Some("printf hello"));
    hub.stop();
}

#[test]
fn a_pane_a_client_opened_is_left_unnamed_by_the_session() {
    // That client named it, or nothing did — either way the hub has nothing to
    // add, and stamping a name here would override a title the client chose.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = TerminalHub::spawn(&dir.path().to_string_lossy(), Vec::new());
    let session = hub.connect();

    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

    let created = next_matching(&session, |f| created_pane(f).is_some()).expect("no pane");
    assert_eq!(created_title(&created), None);
    hub.stop();
}
