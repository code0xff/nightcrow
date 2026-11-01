//! A page on the other end of a real socket: what keeps it open.

use super::super::terminal_connection::{WAKES, run};
use super::super::terminal_liveness::Timing;
use super::super::terminal_pipe::{GUARD_WAKES, READ_GUARD};
use super::fixture::{Wired, short, wired, wired_with};
use crate::session::terminal::{CLIENT_QUEUE_DEPTH, TerminalFrame};
use std::net::Shutdown;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tungstenite::Message;

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

/// A page that is answering but not reading has its frames queued behind a
/// stalled write. Closing it as silent would punish exactly the slow connection
/// the retry beat exists for; the hub's overflow is what ends a client that
/// really stopped keeping up.
#[test]
fn a_stalled_write_is_not_mistaken_for_a_silent_page() {
    let Wired {
        mut page,
        served,
        link,
        session,
        hub,
        cut_off,
        ..
    } = wired();

    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    let runner = std::thread::spawn(move || {
        run(served, link, session, short());
        flag.store(true, Ordering::Release);
    });

    // Fill the socket so the writer stalls. Fewer frames than the hub's queue
    // can hold, so overflow cannot end this connection and the silence limit is
    // the only thing that could.
    let data: Arc<[u8]> = vec![b'x'; 256 * 1024].into();
    for _ in 0..CLIENT_QUEUE_DEPTH - 6 {
        hub.test_broadcast(TerminalFrame::Output {
            pane: 1,
            data: Arc::clone(&data),
        });
    }

    // The page is alive on the way in and never reads. Those frames reach the
    // connection's channel and stop there, because the writer may not drain it
    // while a write is outstanding.
    let typing = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_millis(800);
        while Instant::now() < until {
            if page
                .send(Message::Text(r#"{"type":"zoom","pane":null}"#.into()))
                .is_err()
            {
                return None;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        Some(page)
    });

    std::thread::sleep(Duration::from_millis(800));
    assert!(
        !finished.load(Ordering::Acquire),
        "a stalled socket was closed as though the page had gone silent"
    );
    assert_eq!(hub.client_count(), 1, "the client must still be holding on");

    let _ = cut_off.shutdown(Shutdown::Both);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !finished.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        finished.load(Ordering::Acquire),
        "the cut-off socket did not end the loop"
    );
    runner.join().expect("the connection thread panicked");
    let _page = typing.join().expect("the typing page panicked");
    assert_eq!(hub.client_count(), 0);
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
