//! Timed state the worker used to discover on a tick: the size-owner release
//! grace, a synchronized update the program never closed, the trailing snapshot
//! an idle pane still owes, and a drain that stopped with events still queued.
//!
//! Each runs against a real child on a real PTY, because what is under test is
//! the worker coming back *without* another byte arriving. The fixtures stay
//! alive and silent after their paint so nothing but a clock can move them; the
//! burst leaves a queue already full, so nothing but the drain deadline can.

use super::pty_fixture::{fixture_hub, read_through, start_pane};
use super::{
    SHELL_TEST_DEADLINE, attach, created_pane, created_title, next_matching, spawn_hub,
    wait_until_parked,
};
use crate::backend::PaneId;
use crate::session::size_owner::RELEASE_GRACE;
use crate::session::terminal::TerminalSession;
use crate::session::terminal::frame::{ClientMessage, TerminalFrame};
use std::thread;
use std::time::{Duration, Instant};

/// Past vte's 150 ms window for an open synchronized update, with room for a
/// loaded machine to schedule the pass that ends it.
const PAST_SYNC_WINDOW: Duration = Duration::from_millis(400);

/// Far past any test's deadline: with the worker's cap out of reach, only the
/// deadline a piece of timed state computes can end its wait, so a test of that
/// deadline fails rather than passing on the cap.
const HIGH_CAP: Duration = Duration::from_secs(30);

/// `BSU`, then a title, then silence: the title is inside the update, so only the
/// timeout can end the update and let the tracking emulator read it.
#[cfg(unix)]
const HOLD_UPDATE: &str = "printf '\\033[?2026h\\033]0;held-title\\007'; sleep 5";
#[cfg(windows)]
const HOLD_UPDATE: &str = concat!(
    "$e=[char]27; [Console]::Write($e+'[?2026h'); ",
    "[Console]::Write($e+']0;held-title'+[char]7); Start-Sleep -Seconds 5"
);

/// Alternate-screen paint, then a wait for a line of input, then one more write,
/// then silence. The gated tail lets the pass that stores the entry screen finish
/// first, so the window the tail's snapshot waits out starts from that pass and the
/// child itself produces nothing else.
#[cfg(unix)]
const PAINT_THEN_WAIT: &str =
    "printf '\\033[?1049h\\033[2J\\033[1;1HHEADER'; read line; printf '\\033[2;1HTAIL'; sleep 5";
#[cfg(windows)]
const PAINT_THEN_WAIT: &str = concat!(
    "$e=[char]27; [Console]::Write($e+'[?1049h'+$e+'[2J'+$e+'[1;1HHEADER'); ",
    "[Console]::ReadLine() | Out-Null; ",
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

/// A burst larger than one drain window's budget, then a marker, then the end of
/// the child. Once the reader goes quiet, the events behind the window's budget
/// have no further signal to bring the worker back. Kept small on Unix: macOS
/// hands PTY output over in small chunks, and a debug build there moves a larger
/// burst too slowly for the deadline.
#[cfg(unix)]
const BURST_THEN_MARKER: &str =
    "head -c 524288 /dev/zero | tr '\\0' 'x'; printf 'nightcrow-burst-done'";
#[cfg(windows)]
const BURST_THEN_MARKER: &str = concat!(
    "$out=[Console]::OpenStandardOutput(); ",
    "$bytes=[Text.Encoding]::ASCII.GetBytes(('x' * 2097152)); ",
    "$out.Write($bytes,0,$bytes.Length); $out.Flush(); ",
    "$marker=[Text.Encoding]::ASCII.GetBytes('nightcrow-burst-done'); ",
    "$out.Write($marker,0,$marker.Length); $out.Flush()"
);

const BURST_MARKER: &[u8] = b"nightcrow-burst-done";

/// How long the burst may take to reach the client. Generous for slow hosted
/// runners, yet under the 30 s cap the test puts in the way, so a missing drain
/// deadline still fails rather than being rescued by the cap.
const BURST_DEADLINE: Duration = Duration::from_secs(25);

#[test]
fn the_size_owner_grace_ends_on_the_worker_clock() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = spawn_hub(&dir.path().to_string_lossy(), Vec::new(), Vec::new());
    hub.set_wait_cap(HIGH_CAP);
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
    let hub = fixture_hub(&dir.path().to_string_lossy(), PAINT_THEN_WAIT);
    hub.set_wait_cap(HIGH_CAP);
    let (session, pane) = start_pane(&hub);

    // The entry paint, and then the child waits on its input. Wait until the pane's
    // screen is stored: the snapshot window the tail waits out starts from that
    // pass, so the tail lands well inside it rather than on a screen taken with it.
    read_through(&session, pane, b"HEADER");
    let deadline = Instant::now() + SHELL_TEST_DEADLINE;
    let mut stored = false;
    while Instant::now() < deadline {
        let (screen, since) = hub.pane_record(pane).expect("the pane is live");
        if screen > 0 && since == 0 {
            stored = true;
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        stored,
        "the entry screen never reached a stored snapshot: a paced pass never came \
         back for it"
    );

    // The tail lands on top of that screen and the child is silent again: only a
    // paced pass can fold it in.
    session.dispatch(ClientMessage::Input {
        pane,
        data: "go\r".to_string(),
    });

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

#[test]
fn a_drain_left_over_at_its_budget_does_not_wait_for_a_signal() {
    let dir = tempfile::TempDir::new().unwrap();
    let hub = fixture_hub(&dir.path().to_string_lossy(), BURST_THEN_MARKER);
    // The events a pass leaves queued have no producer left to signal for them,
    // so a cap the test could sit out would hide the missing drain deadline.
    hub.set_wait_cap(HIGH_CAP);
    let (session, pane) = start_pane(&hub);

    let took = read_burst(&session, pane);
    assert!(
        took < BURST_DEADLINE,
        "the burst's tail took {took:?} to reach the client: the drain waited out \
         the {HIGH_CAP:?} cap instead of coming back for the events it left queued"
    );
    hub.stop();
}

/// Read `session` until the burst's marker arrives, keeping only a short tail and
/// answering the cursor queries a PowerShell child waits on: a multi-megabyte
/// burst must not be rescanned on every frame. Returns when the deadline passes
/// without the marker, so the caller owns the verdict.
fn read_burst(session: &TerminalSession, pane: PaneId) -> Duration {
    let started = Instant::now();
    let mut tail: Vec<u8> = Vec::new();
    let mut answered = 0;
    while started.elapsed() < BURST_DEADLINE {
        let Some(frame) = session.next_frame(Duration::from_millis(100)) else {
            continue;
        };
        let TerminalFrame::Output {
            pane: event_pane,
            data,
        } = frame
        else {
            continue;
        };
        if event_pane != pane {
            continue;
        }
        let asked = data
            .windows(4)
            .filter(|window| *window == b"\x1b[6n")
            .count();
        while answered < asked {
            session.dispatch(ClientMessage::Input {
                pane,
                data: "\x1b[1;1R".to_string(),
            });
            answered += 1;
        }
        tail.extend_from_slice(&data);
        if tail.len() > BURST_MARKER.len() * 2 {
            let excess = tail.len() - BURST_MARKER.len() * 2;
            tail.drain(..excess);
        }
        if tail
            .windows(BURST_MARKER.len())
            .any(|window| window == BURST_MARKER)
        {
            return started.elapsed();
        }
    }
    started.elapsed()
}
