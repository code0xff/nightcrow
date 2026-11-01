//! The two helper threads behind one terminal socket.
//!
//! A connection has three things to wait for — the page's bytes, the hub's
//! frames, and the liveness clock — and the first can only be waited on with a
//! blocking read. So the socket's read half moves to its own thread, the hub
//! queue is drained by another, and both hand what they get to the writer
//! through one bounded channel. The writer then sleeps until an event or the
//! next liveness decision, instead of waking on a timer to find neither.
//!
//! Writes stay in one thread. The WebSocket is parsed and written there alone,
//! so a control frame and a burst of output can never interleave on the wire.

use crate::session::terminal::{FrameWait, TerminalFrame, TerminalSession};
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TryRecvError, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;

/// Events a connection may have in flight before a producer waits. Bytes are
/// read in chunks, so this bounds both directions: the reader stops pulling
/// from the socket once the writer falls this far behind, and the hub queue
/// behind the forwarder fills where the hub's own cap lives.
pub(super) const QUEUE_DEPTH: usize = 8;

/// How much one read may take off the socket.
const READ_CHUNK: usize = 8 * 1024;

/// How long one blocking read may stay unanswered before the reader looks at
/// the stop flag again.
///
/// A blocked read cannot be cancelled portably: shutting the socket down does
/// not wake a `read` already in progress on Windows (measured on this
/// platform). So the reader parks with this guard and costs one wake per
/// interval on an idle connection, which is what lets the connection end
/// instead of leaving the thread behind.
pub(super) const READ_GUARD: Duration = Duration::from_secs(1);

/// A safety net, not an interval: a hub that lets this client go closes the
/// queue, which ends a blocked wait on its own. This only bounds a thread whose
/// sender somehow outlived the hub.
const FRAME_WAIT: Duration = Duration::from_secs(3600);

/// Something for the writer to do.
pub(super) enum Event {
    /// Bytes the page sent, as they came off the socket.
    Bytes(Vec<u8>),
    /// A frame the hub queued for this client.
    Frame(TerminalFrame),
    /// The page's side of the connection ended.
    ClientGone,
}

/// What waiting for the next event produced.
pub(super) enum Waiting {
    Event(Event),
    /// Nothing now; more may arrive.
    Empty,
    /// No producer is left, so nothing more will ever arrive.
    Closed,
}

/// The read half of one terminal socket, the threads feeding the writer, and
/// the handle that ends them.
///
/// Everything a connection owns lives here so that dropping it, however the
/// writer ends, releases all of it (see [`Drop`]). The join happens on the
/// writer's own thread, which never joins itself.
pub(super) struct Link {
    events: Option<Receiver<Event>>,
    reader: Option<JoinHandle<()>>,
    frames: Option<JoinHandle<()>>,
    /// A third handle to the socket, for telling the page the connection is
    /// over even if the writer's half never gets the chance.
    cut_off: TcpStream,
    /// Set when the connection is ending, so the reader stops instead of
    /// waiting out another guard interval.
    stop: Arc<AtomicBool>,
    /// The client this connection speaks for, so ending it can release the hub
    /// record and with it the frame thread's wait.
    session: Arc<TerminalSession>,
    /// Panic on the writer's first pass. Test-only: it proves that unwinding
    /// still releases the hub client and joins the helper threads.
    #[cfg(test)]
    pub(super) panic_writer: bool,
}

impl Link {
    /// Start both threads. `socket` is the read half: the caller keeps the
    /// write half inside the WebSocket, which is why a clone is needed at all.
    pub(super) fn open(
        socket: TcpStream,
        session: &Arc<TerminalSession>,
        guard: Duration,
    ) -> io::Result<Self> {
        let cut_off = socket.try_clone()?;
        socket.set_read_timeout(Some(guard))?;
        let (tx, events) = sync_channel(QUEUE_DEPTH);
        let stop = Arc::new(AtomicBool::new(false));
        let reader = spawn_reader(socket, tx.clone(), Arc::clone(&stop))?;
        let frames = match spawn_frames(Arc::clone(session), tx) {
            Ok(frames) => frames,
            Err(err) => {
                // The connection cannot be served; end the reader now rather
                // than leave it parked on a socket nobody will read again.
                stop.store(true, Ordering::Release);
                let _ = cut_off.shutdown(Shutdown::Both);
                drop(events);
                let _ = reader.join();
                return Err(err);
            }
        };
        Ok(Self {
            events: Some(events),
            reader: Some(reader),
            frames: Some(frames),
            cut_off,
            stop,
            session: Arc::clone(session),
            #[cfg(test)]
            panic_writer: false,
        })
    }

    /// Wait up to `timeout` for the next event.
    pub(super) fn next(&self, timeout: Duration) -> Waiting {
        match self
            .events
            .as_ref()
            .expect("a running link has a receiver")
            .recv_timeout(timeout)
        {
            Ok(event) => Waiting::Event(event),
            Err(RecvTimeoutError::Timeout) => Waiting::Empty,
            Err(RecvTimeoutError::Disconnected) => Waiting::Closed,
        }
    }

    /// The next event if one is already waiting, so a burst leaves in one
    /// flush instead of one syscall pair per frame.
    pub(super) fn try_next(&self) -> Waiting {
        match self
            .events
            .as_ref()
            .expect("a running link has a receiver")
            .try_recv()
        {
            Ok(event) => Waiting::Event(event),
            Err(TryRecvError::Empty) => Waiting::Empty,
            Err(TryRecvError::Disconnected) => Waiting::Closed,
        }
    }
}

impl Drop for Link {
    /// End both threads and let the hub release the client, on every path.
    ///
    /// A guard rather than a call the writer makes when it is done: a panic in
    /// the writer unwinds without reaching the end of its loop, and that is
    /// exactly the case where a leaked reader thread and hub client would
    /// outlive the socket. `session.close` is idempotent, and the stop flag plus
    /// the read guard end the reader even though a read in progress cannot be
    /// interrupted from here.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.session.close();
        let _ = self.cut_off.shutdown(Shutdown::Both);
        // Dropping the receiver first is what makes a producer waiting on a
        // full queue give up instead of waiting for a writer that has stopped.
        drop(self.events.take());
        if let Some(reader) = self.reader.take() {
            if let Err(err) = reader.join() {
                tracing::debug!(?err, "viewer: terminal reader thread panicked");
            }
            #[cfg(test)]
            JOINS.fetch_add(1, Ordering::Relaxed);
        }
        if let Some(frames) = self.frames.take() {
            if let Err(err) = frames.join() {
                tracing::debug!(?err, "viewer: terminal frame thread panicked");
            }
            #[cfg(test)]
            JOINS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Count the helper threads a connection has joined, so a test can show that an
/// unwinding writer still waits for both.
#[cfg(test)]
pub(super) static JOINS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn spawn_reader(
    mut socket: TcpStream,
    events: SyncSender<Event>,
    stop: Arc<AtomicBool>,
) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("nightcrow-term-read".into())
        .spawn(move || {
            let mut buf = vec![0u8; READ_CHUNK];
            while !stop.load(Ordering::Acquire) {
                match socket.read(&mut buf) {
                    // A zero-length read is the page going away without a close
                    // frame; there is nothing left to parse.
                    Ok(0) => break,
                    Ok(n) => {
                        // Blocking, not dropping: the socket may hold more, and
                        // TCP backpressure is what should slow the page down.
                        if events.send(Event::Bytes(buf[..n].to_vec())).is_err() {
                            return;
                        }
                    }
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    // The guard interval, or a socket that has nothing yet.
                    Err(err)
                        if matches!(
                            err.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                        ) =>
                    {
                        #[cfg(test)]
                        GUARD_WAKES.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(_) => break,
                }
            }
            let _ = events.send(Event::ClientGone);
        })
}

/// Count the reader's guard-interval wake-ups, so the idle measurement can show
/// what an idle connection still costs.
#[cfg(test)]
pub(super) static GUARD_WAKES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn spawn_frames(
    session: Arc<TerminalSession>,
    events: SyncSender<Event>,
) -> io::Result<JoinHandle<()>> {
    std::thread::Builder::new()
        .name("nightcrow-term-frames".into())
        .spawn(move || {
            loop {
                match session.wait_frame(FRAME_WAIT) {
                    FrameWait::Frame(frame) => {
                        if events.send(Event::Frame(frame)).is_err() {
                            return;
                        }
                    }
                    FrameWait::Idle => {}
                    // The hub let this client go; nothing more will arrive.
                    FrameWait::Closed => return,
                }
            }
        })
}

/// Writes go straight to the connection; reads come from bytes the reader
/// thread already took off it. Handing the writer's socket to the WebSocket and
/// the reader's clone to the reader keeps each direction in one thread.
pub(super) struct Duplex {
    socket: TcpStream,
    buffered: VecDeque<u8>,
}

impl Duplex {
    pub(super) fn new(socket: TcpStream) -> Self {
        Self {
            socket,
            buffered: VecDeque::new(),
        }
    }

    /// Offer the reader's bytes to the WebSocket's parser.
    pub(super) fn feed(&mut self, bytes: &[u8]) {
        self.buffered.extend(bytes.iter().copied());
    }

    /// Shut the connection down. Any handle to the socket does this for all of
    /// them, which is also how the hub ends a client that stopped keeping up.
    pub(super) fn cut_off(&self) {
        let _ = self.socket.shutdown(Shutdown::Both);
    }
}

impl Read for Duplex {
    /// `WouldBlock` with nothing buffered is the parser asking for bytes that
    /// have not arrived; tungstenite treats it as a stall and keeps its decode
    /// state, so the next `feed` resumes exactly where this stopped.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.buffered.is_empty() {
            return Err(io::Error::from(io::ErrorKind::WouldBlock));
        }
        let taken = buf.len().min(self.buffered.len());
        for (slot, byte) in buf.iter_mut().zip(self.buffered.drain(..taken)) {
            *slot = byte;
        }
        Ok(taken)
    }
}

impl Write for Duplex {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.socket.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.socket.flush()
    }
}

#[cfg(test)]
#[path = "terminal_pipe_tests.rs"]
mod tests;
