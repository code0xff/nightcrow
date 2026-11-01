//! A page on the other end of a real socket: what keeps it, and what ends it.

use super::super::terminal_connection::{WAKES, run};
use super::super::terminal_liveness::Timing;
use super::super::terminal_pipe::{GUARD_WAKES, JOINS, READ_GUARD};
use super::fixture::{Wired, short, wired, wired_panicking, wired_with};
use crate::session::terminal::TerminalFrame;
use std::net::Shutdown;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tungstenite::Message;

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

/// The page's browser answers pings on its own; the connection must survive
/// well past the silence limit as proof that it did.
#[test]
fn an_answered_ping_keeps_the_socket_open() {
    let Wired {
        mut page,
        served,
        link,
        session,
        hub,
        ..
    } = wired();
    let answering = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_millis(900);
        while Instant::now() < until {
            match page.read() {
                Ok(Message::Ping(payload)) => {
                    if page.send(Message::Pong(payload)).is_err() {
                        return;
                    }
                }
                Ok(Message::Close(_)) => return,
                Ok(_) => {}
                Err(tungstenite::Error::Io(err))
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return,
            }
        }
        let _ = page.close(None);
    });

    let started = Instant::now();
    run(served, link, session, short());
    let elapsed = started.elapsed();
    answering.join().expect("the answering page panicked");

    assert!(
        elapsed >= Duration::from_millis(700),
        "answered pings did not keep the socket open: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the loop did not notice the page leaving: {elapsed:?}"
    );
    hub.stop();
}

/// A pong is not the only proof: ordinary input arriving is the same evidence,
/// and a page that types but never answers a ping must not be cut off.
#[test]
fn ordinary_input_keeps_the_socket_open() {
    let Wired {
        mut page,
        served,
        link,
        session,
        hub,
        ..
    } = wired();
    let typing = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_millis(900);
        while Instant::now() < until {
            if page
                .send(Message::Text(r#"{"type":"zoom","pane":null}"#.into()))
                .is_err()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = page.close(None);
    });

    let started = Instant::now();
    run(served, link, session, short());
    let elapsed = started.elapsed();
    typing.join().expect("the typing page panicked");

    assert!(
        elapsed >= Duration::from_millis(700),
        "input did not count as the page still being there: {elapsed:?}"
    );
    hub.stop();
}

/// A page watching a busy pane sends nothing of its own: no typing, no input,
/// only the pongs it answers automatically. If output postpones the ping, such
/// a page never gets asked, never answers, and is closed as silent on every
/// silence limit — replaying every pane for a tab that was there all along.
#[test]
fn a_page_watching_a_stream_still_answers_pings_and_stays_open() {
    let Wired {
        mut page,
        served,
        link,
        session,
        hub,
        ..
    } = wired();

    let reading = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_millis(900);
        while Instant::now() < until {
            match page.read() {
                Ok(Message::Ping(payload)) => {
                    if page.send(Message::Pong(payload)).is_err() {
                        return;
                    }
                }
                Ok(Message::Close(_)) => return,
                Ok(_) => {}
                Err(tungstenite::Error::Io(err))
                    if matches!(
                        err.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => return,
            }
        }
        let _ = page.close(None);
    });

    let streaming = {
        let hub = Arc::clone(&hub);
        std::thread::spawn(move || {
            let data: Arc<[u8]> = b"streaming output".to_vec().into();
            let until = Instant::now() + Duration::from_millis(900);
            while Instant::now() < until {
                hub.test_broadcast(TerminalFrame::Output {
                    pane: 1,
                    data: Arc::clone(&data),
                });
                std::thread::sleep(Duration::from_millis(50));
            }
        })
    };

    let started = Instant::now();
    run(served, link, session, short());
    let elapsed = started.elapsed();
    reading.join().expect("the reading page panicked");
    streaming.join().expect("the streaming thread panicked");

    assert!(
        elapsed >= Duration::from_millis(800),
        "output alone left the page unpinged, so it was closed as silent: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "the loop did not notice the page leaving: {elapsed:?}"
    );
    assert_eq!(
        hub.client_count(),
        0,
        "a page that left must give up its hub client"
    );
    hub.stop();
}

/// A page that never reads must not be able to make the writer hold its frames:
/// once a write stalls, the hub's own bounded queue fills and its overflow is
/// what ends the connection, long before the silence limit would.
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

    // Large frames fill the loopback socket quickly, so the writer is still
    // stalled while the hub has a backlog to hand it.
    let data: Arc<[u8]> = vec![b'x'; 256 * 1024].into();
    let started = Instant::now();
    while hub.client_count() > 0 && started.elapsed() < Duration::from_secs(5) {
        hub.test_broadcast(TerminalFrame::Output {
            pane: 1,
            data: Arc::clone(&data),
        });
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(
        hub.client_count(),
        0,
        "the hub must cut off a page that stopped reading, instead of the writer buffering for it"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the overflow took longer than the whole test budget: {:?}",
        started.elapsed()
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

/// Not a pass/fail assertion: it reports what one idle socket still costs in
/// wake-ups per second. The writer's count is its loop passes; the reader's is
/// its guard-interval wake-ups, the price of being able to end a read that
/// cannot be interrupted.
#[test]
#[ignore = "measurement: prints wakeups instead of asserting them"]
fn idle_socket_wakeups_in_one_second() {
    let Wired {
        served,
        link,
        session,
        cut_off,
        hub,
        page: _page,
        ..
    } = wired_with(READ_GUARD);
    WAKES.store(0, Ordering::Relaxed);
    GUARD_WAKES.store(0, Ordering::Relaxed);
    let runner = std::thread::spawn(move || {
        run(
            served,
            link,
            session,
            Timing {
                ping_after: Duration::from_secs(60),
                silence_limit: Duration::from_secs(120),
            },
        );
    });
    std::thread::sleep(Duration::from_secs(1));
    let writer = WAKES.load(Ordering::Relaxed);
    let reader = GUARD_WAKES.load(Ordering::Relaxed);
    let _ = cut_off.shutdown(Shutdown::Both);
    runner.join().expect("the connection thread panicked");

    println!("idle socket wakeups in 1 s: writer {writer}, reader {reader}");
    hub.stop();
}
