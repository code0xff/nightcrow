//! A page on the other end of a real socket: what ends it, and what it releases.

use super::super::terminal_connection::{DEQUEUED_WHILE_STALLED, run};
use super::super::terminal_liveness::Timing;
use super::super::terminal_pipe::JOINS;
use super::fixture::{Wired, short, wired, wired_panicking};
use crate::session::terminal::{CLIENT_QUEUE_DEPTH, TerminalFrame};
use std::net::Shutdown;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// No read, no write, no pong: the page's network path is gone even though its
/// socket still exists on this side.
#[test]
fn a_page_that_stops_answering_is_closed() {
    let wired = wired();
    let started = Instant::now();
    run(wired.served, wired.link, wired.session, short());
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(400),
        "the socket was closed before the page had a chance to answer: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the loop never woke to notice: {elapsed:?}"
    );
    assert_eq!(
        wired.hub.client_count(),
        0,
        "a closed socket must give up its hub client and with it the connection slot"
    );
    wired.hub.stop();
}

/// A page that never reads must not be able to make the writer hold its frames:
/// once a write stalls the writer takes nothing more from the hub, so the hub's
/// own bounded queue fills and its overflow is what ends the connection, long
/// before the silence limit would.
#[test]
fn a_page_that_never_reads_is_cut_off_by_the_hub() {
    let Wired {
        served,
        link,
        session,
        hub,
        page: _page,
        ..
    } = wired();

    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    let runner = std::thread::spawn(move || {
        run(served, link, session, Timing::default());
        flag.store(true, Ordering::Release);
    });

    DEQUEUED_WHILE_STALLED.store(0, Ordering::Relaxed);
    // Fill the queue just short of its cap. A frame large enough that the
    // socket takes only a handful before stalling keeps the queue this full.
    let data: Arc<[u8]> = vec![b'x'; 1024 * 1024].into();
    for _ in 0..CLIENT_QUEUE_DEPTH - 2 {
        hub.test_broadcast(TerminalFrame::Output {
            pane: 1,
            data: Arc::clone(&data),
        });
    }
    assert_eq!(hub.client_count(), 1, "the fill itself must fit the queue");

    // A writer that kept draining the hub after the stall would empty a good
    // part of that backlog here; one that stopped cannot.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        DEQUEUED_WHILE_STALLED.load(Ordering::Relaxed),
        0,
        "no frame may leave the hub while a write is stalled"
    );
    assert_eq!(
        hub.client_count(),
        1,
        "the writer must not leave the hub short of work to end it"
    );

    // The remaining frames now fit only if the queue is still full. The hub's
    // overflow, not the writer's buffer, is what has to end this client.
    for _ in 0..40 {
        hub.test_broadcast(TerminalFrame::Output {
            pane: 1,
            data: Arc::clone(&data),
        });
    }

    assert_eq!(
        hub.client_count(),
        0,
        "the hub must cut off a page that stopped reading, instead of the writer buffering for it"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    while !finished.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        finished.load(Ordering::Acquire),
        "the connection did not end after the hub cut it off"
    );
    runner.join().expect("the connection thread panicked");
    hub.stop();
}

/// A panic in the writer must still release the hub client and join both helper
/// threads: `run` owns them, and an unwind never reaches the end of its loop.
#[test]
fn a_panicking_writer_still_releases_the_connection() {
    let before = JOINS.load(Ordering::Relaxed);
    let Wired {
        served,
        link,
        session,
        hub,
        ..
    } = wired_panicking();

    let runner = std::thread::spawn(move || run(served, link, session, Timing::default()));
    assert!(
        runner.join().is_err(),
        "the writer was supposed to panic, so the loop did not end"
    );

    assert_eq!(
        hub.client_count(),
        0,
        "a panicking writer must still give up its hub client"
    );
    assert!(
        JOINS.load(Ordering::Relaxed) >= before + 2,
        "both helper threads must be joined on the way out"
    );
    hub.stop();
}

#[test]
fn a_page_that_closes_its_socket_ends_the_loop_at_once() {
    let Wired {
        mut page,
        served,
        link,
        session,
        hub,
        ..
    } = wired();
    page.close(None).expect("a close frame");

    let started = Instant::now();
    // The production timings, so nothing but the close frame can end this.
    run(served, link, session, Timing::default());

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a page that said goodbye was kept waiting"
    );
    assert_eq!(hub.client_count(), 0);
    hub.stop();
}

/// A page that dropped its connection without a close frame: the reader sees
/// the end and the writer must follow it, rather than wait out the silence.
#[test]
fn a_page_that_vanishes_without_a_close_ends_the_loop() {
    let Wired {
        page,
        served,
        link,
        session,
        hub,
        ..
    } = wired();
    drop(page);

    let started = Instant::now();
    run(served, link, session, Timing::default());

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a vanished page was left waiting"
    );
    assert_eq!(hub.client_count(), 0);
    hub.stop();
}

/// What the hub does to a client that stops draining its queue: it shuts the
/// socket down through the handle it was given. The connection must end on its
/// own, releasing the hub client, without the page saying anything.
#[test]
fn a_connection_cut_off_by_the_hub_ends_the_loop() {
    let Wired {
        served,
        link,
        session,
        cut_off,
        hub,
        page: _page,
        ..
    } = wired();
    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    let runner = std::thread::spawn(move || {
        run(served, link, session, Timing::default());
        flag.store(true, Ordering::Release);
    });

    std::thread::sleep(Duration::from_millis(100));
    assert!(
        !finished.load(Ordering::Acquire),
        "the loop ended before anything cut it off"
    );
    cut_off
        .shutdown(Shutdown::Both)
        .expect("could not cut the connection off");
    runner.join().expect("the connection thread panicked");

    // `run` joins both helper threads before it returns, so a finished runner
    // is also the proof that neither was left behind.
    assert_eq!(hub.client_count(), 0, "a cut-off client must be released");
    hub.stop();
}
