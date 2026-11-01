//! The stream seam between the writer's socket and the bytes the reader took
//! off it.
//!
//! The two directions run on different threads, so each half of the socket has
//! one owner. Reads come from a buffer the reader thread fills; writes go
//! straight out. Handing tungstenite this type keeps the parser out of the
//! read syscall without giving it a second thread of its own.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};

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
