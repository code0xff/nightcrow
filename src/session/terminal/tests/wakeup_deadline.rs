//! Timed state the worker used to discover on a tick: the size-owner release
//! grace, a synchronized update the program never closed, and the trailing
//! snapshot an idle pane still owes.
//!
//! Each runs against a real child on a real PTY, because what is under test is
//! the worker coming back *without* another byte arriving. The fixtures stay
//! alive and silent after their paint so nothing but a clock can move them.

use super::pty_fixture::{fixture_hub, read_through, start_pane};
use super::{
    SHELL_TEST_DEADLINE, attach, created_pane, created_title, next_matching, spawn_hub,
    wait_until_parked,
};
use crate::session::size_owner::RELEASE_GRACE;
use crate::session::terminal::frame::TerminalFrame;
use std::thread;
use std::time::{Duration, Instant};

/// Past vte's 150 ms window for an open synchronized update, with room for a
/// loaded machine to schedule the pass that ends it.
const PAST_SYNC_WINDOW: Duration = Duration::from_millis(400);

/// `BSU`, then a title, then silence: the title is inside the update, so only the
/// timeout can end the update and let the tracking emulator read it.
#[cfg(unix)]
const HOLD_UPDATE: &str = "printf '\\033[?2026h\\033]0;held-title\\007'; sleep 5";
#[cfg(windows)]
const HOLD_UPDATE: &str = concat!(
    "$e=[char]27; [Console]::Write($e+'[?2026h'); ",
    "[Console]::Write($e+']0;held-title'+[char]7); Start-Sleep -Seconds 5"
);

/// Alternate-screen paint, a pause well inside the snapshot pace's window, one
/// more write, then silence. The second write lands on top of the entry screen
/// and stays owed until the window passes.
#[cfg(unix)]
const PAINT_THEN_IDLE: &str =
    "printf '\\033[?1049h\\033[2J\\033[1;1HHEADER'; sleep 0.1; printf '\\033[2;1HTAIL'; sleep 5";
#[cfg(windows)]
const PAINT_THEN_IDLE: &str = concat!(
    "$e=[char]27; [Console]::Write($e+'[?1049h'+$e+'[2J'+$e+'[1;1HHEADER'); ",
    "Start-Sleep -Milliseconds 100; ",
    "[Console]::Write($e+'[2;1HTAIL'); Start-Sleep -Seconds 5"
);

/// Whether a frame is a size-ownership verdict, and what it says.
fn owned_verdict(frame: &TerminalFrame) -> Option<bool> {
    let TerminalFrame::Control(json) = frame else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    if value["type"] != "size_owner" {
        return None;
    }
    value["owned"].as_bool()
}

#[test]
fn the_size_owner_grace_ends_on_the_worker_clock() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    let owner = attach(&hub);
    let heir = attach(&hub);
    let passive = next_matching(&heir, |f| owned_verdict(f).is_some()).expect("no verdict");
    assert_eq!(
        owned_verdict(&passive),
        Some(false),
        "a later arrival waits for direct input"
    );
    // Nothing but the grace is running: no pane, no output, no client traffic.
    wait_until_parked(&hub);

    let started = Instant::now();
    drop(owner);
    let release = next_matching(&heir, |f| owned_verdict(f) == Some(true));
    let waited = started.elapsed();
    assert!(release.is_some(), "the grace never ended");
    assert!(
        waited >= RELEASE_GRACE,
        "ownership moved after {waited:?}, inside the grace"
    );
    assert!(
        waited < RELEASE_GRACE + Duration::from_millis(500),
        "ownership moved after {waited:?}, long past the grace"
    );
    hub.stop();
}

#[test]
fn an_open_synchronized_update_is_read_once_its_timeout_passes() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = fixture_hub(&dir.path().to_string_lossy(), HOLD_UPDATE);
    let (painted, pane) = start_pane(&hub);

    // The update's opening bytes. Nothing follows them, so only the timeout can
    // end the update and let the hub read the title it is holding.
    read_through(&painted, pane, b"\x1b[?2026h\x1b]0;");
    thread::sleep(PAST_SYNC_WINDOW);

    let late = attach(&hub);
    let created = next_matching(&late, |f| created_pane(f) == Some(pane))
        .expect("the late client was not replayed the pane");
    assert_eq!(
        created_title(&created).as_deref(),
        Some("held-title"),
        "the update's title was never read"
    );
    hub.stop();
}

#[test]
fn an_idle_pane_owing_bytes_gets_its_trailing_snapshot() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = fixture_hub(&dir.path().to_string_lossy(), PAINT_THEN_IDLE);
    let (session, pane) = start_pane(&hub);

    read_through(&session, pane, b"TAIL");

    // The child is quiet from here on: the entry screen was stored before this
    // write, so only the paced pass can fold it in.
    let mut owed_seen = false;
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    while Instant::now() < deadline {
        let (_, since) = hub.pane_record(pane).expect("the pane is live");
        if since > 0 {
            owed_seen = true;
        } else if owed_seen {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let (screen, since) = hub.pane_record(pane).expect("the pane is live");
    assert!(
        owed_seen,
        "the tail was never recorded on top of the entry screen"
    );
    assert_eq!(since, 0, "the owed tail was never folded into a snapshot");
    assert!(screen > 0, "the record must carry a screen");
    hub.stop();
}
