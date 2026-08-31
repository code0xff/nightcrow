//! The startup terminals: offered to be sized, created once, named by the
//! session.
//!
//! Their own file because they are a handshake rather than a pane operation —
//! the hub holds them until a client measures the cells, and the claim is what
//! makes that happen exactly once for the hub's life.

use super::{
    attach, collect_created, created_pane, created_size, created_title, next_matching,
    pending_count, spawn_hub, spawn_hub_with_auto_open,
};
use crate::config::StartupCommand;
use crate::session::limits;
use crate::session::terminal::frame::{ClientMessage, PaneSize, TerminalFrame};

/// A startup terminal configured with no name of its own, which is what most of
/// these tests care about — they assert on panes, not on what they are called.
fn startup(command: &str) -> StartupCommand {
    StartupCommand {
        name: None,
        command: command.to_string(),
        plugin: None,
    }
}

#[test]
fn a_startup_terminal_is_offered_for_sizing_and_born_at_that_size() {
    // The whole point of the handshake: the child must never draw a frame at a
    // size no client chose, so the PTY does not exist until one has measured.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub_with_auto_open(
        &dir.path().to_string_lossy(),
        vec![startup("printf hello")],
        Vec::new(),
        false,
    );
    let session = attach(&hub);

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
fn a_startup_size_of_zero_is_clamped_rather_than_reaching_openpty() {
    // A zero dimension can fail `openpty` outright, and the claim is already
    // spent by then — the hub would hold `started` with no terminal to show
    // for it and never offer them again.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = attach(&hub);
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
    let hub = spawn_hub(&dir.path().to_string_lossy(), configured, Vec::new());
    let session = attach(&hub);
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
    let hub = spawn_hub(&dir.path().to_string_lossy(), configured, Vec::new());
    let session = attach(&hub);
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
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());

    let abandoned = attach(&hub);
    assert!(
        next_matching(&abandoned, |f| pending_count(f).is_some()).is_some(),
        "the first client was not offered the startup terminals"
    );
    drop(abandoned);

    let second = attach(&hub);
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
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let first = attach(&hub);
    let second = attach(&hub);

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
    let hub = spawn_hub(
        &dir.path().to_string_lossy(),
        // Long-lived on purpose, unlike the `printf` the tests above use: this
        // one checks what a *later* client is replayed, and a pane whose command
        // has exited is gone from the session by then — under load the exit wins
        // the race and the replay is empty.
        vec![StartupCommand {
            name: Some("Claude".into()),
            command: "sleep 30".into(),
            plugin: None,
        }],
        Vec::new(),
    );
    let session = attach(&hub);
    session.dispatch(ClientMessage::Start {
        sizes: vec![PaneSize { rows: 24, cols: 80 }],
    });

    let created = next_matching(&session, |f| created_pane(f).is_some()).expect("no pane");
    assert_eq!(created_title(&created).as_deref(), Some("Claude"));

    // And a client that connects later is told it too, rather than showing
    // something the other clients do not call it.
    let late = attach(&hub);
    let replayed = next_matching(&late, |f| created_pane(f).is_some()).expect("no replay");
    assert_eq!(created_title(&replayed).as_deref(), Some("Claude"));
    hub.stop();
}

#[test]
fn an_unnamed_startup_terminal_falls_back_to_its_command() {
    // What the operator wrote is what they would recognise it by.
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(
        &dir.path().to_string_lossy(),
        vec![startup("printf hello")],
        Vec::new(),
    );
    let session = attach(&hub);
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
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let session = attach(&hub);

    session.dispatch(ClientMessage::Create { rows: 24, cols: 80 });

    let created = next_matching(&session, |f| created_pane(f).is_some()).expect("no pane");
    assert_eq!(created_title(&created), None);
    hub.stop();
}
