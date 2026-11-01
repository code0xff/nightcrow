//! A page on the other end of a real socket: what keeps it, and what ends it.

use super::{Timing, WAKES, run};
use crate::session::size_owner::ViewerId;
use crate::session::terminal::{TerminalHub, TerminalSession};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tungstenite::Message;
use tungstenite::protocol::{Role, WebSocket};

/// What both ends of one connection look like before the loop starts: the page
/// side, the served side, and the handles the hub and a test use to cut it off.
struct Wired {
    _dir: tempfile::TempDir,
    hub: Arc<TerminalHub>,
    served: WebSocket<TcpStream>,
    page: WebSocket<TcpStream>,
    session: TerminalSession,
    cut_off: TcpStream,
}

fn wired() -> Wired {
    let dir = tempfile::TempDir::new().expect("a temporary directory");
    let hub = TerminalHub::spawn(
        &dir.path().to_string_lossy(),
        Vec::new(),
        Vec::new(),
        false,
        crate::config::ShellConfig::default(),
        Arc::default(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback listener");
    let page_stream = TcpStream::connect(listener.local_addr().unwrap()).expect("a dial");
    let (served_stream, _) = listener.accept().expect("an inbound connection");
    // What `conn::read_request` leaves: no read timeout of its own, and the
    // poll interval `serve_terminal` installs for the connection loop.
    served_stream
        .set_read_timeout(Some(Duration::from_millis(10)))
        .expect("a read timeout");
    served_stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("a write timeout");
    page_stream
        .set_read_timeout(Some(Duration::from_millis(20)))
        .expect("a read timeout");
    let cut_off = served_stream.try_clone().expect("a socket clone");
    let session = hub.connect(
        ViewerId::Browser("liveness-test".to_string()),
        true,
        Some(served_stream.try_clone().expect("a socket clone")),
    );
    Wired {
        _dir: dir,
        hub,
        served: WebSocket::from_raw_socket(served_stream, Role::Server, None),
        page: WebSocket::from_raw_socket(page_stream, Role::Client, None),
        session,
        cut_off,
    }
}

fn short() -> Timing {
    Timing {
        ping_after: Duration::from_millis(100),
        silence_limit: Duration::from_millis(400),
    }
}

/// No read, no write, no pong: the page's network path is gone even though its
/// socket still exists on this side.
#[test]
fn a_page_that_stops_answering_is_closed() {
    let wired = wired();
    let started = Instant::now();
    run(wired.served, wired.session, short());
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
    run(served, session, short());
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
    run(served, session, short());
    let elapsed = started.elapsed();
    typing.join().expect("the typing page panicked");

    assert!(
        elapsed >= Duration::from_millis(700),
        "input did not count as the page still being there: {elapsed:?}"
    );
    hub.stop();
}

#[test]
fn a_page_that_closes_its_socket_ends_the_loop_at_once() {
    let Wired {
        mut page,
        served,
        session,
        hub,
        ..
    } = wired();
    page.close(None).expect("a close frame");

    let started = Instant::now();
    // The production timings, so nothing but the close frame can end this.
    run(served, session, Timing::default());

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "a page that said goodbye was kept waiting"
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
        session,
        cut_off,
        hub,
        page: _page,
        ..
    } = wired();
    let finished = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&finished);
    let runner = std::thread::spawn(move || {
        run(served, session, Timing::default());
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

    assert_eq!(hub.client_count(), 0, "a cut-off client must be released");
    hub.stop();
}

/// Not a pass/fail assertion: it reports how often one idle socket wakes its
/// connection thread in a second. Run it alone and compare the number between
/// the polling loop and the event-driven one.
#[test]
#[ignore = "measurement: prints wakeups instead of asserting them"]
fn idle_socket_wakeups_in_one_second() {
    let Wired {
        served,
        session,
        cut_off,
        hub,
        page: _page,
        ..
    } = wired();
    WAKES.store(0, Ordering::Relaxed);
    let runner = std::thread::spawn(move || {
        run(
            served,
            session,
            Timing {
                ping_after: Duration::from_secs(60),
                silence_limit: Duration::from_secs(120),
            },
        );
    });
    std::thread::sleep(Duration::from_secs(1));
    let wakes = WAKES.load(Ordering::Relaxed);
    let _ = cut_off.shutdown(Shutdown::Both);
    runner.join().expect("the connection thread panicked");

    println!("idle socket wakeups in 1 s: {wakes}");
    hub.stop();
}
