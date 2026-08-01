//! Framing for the daemon socket. A Unix socket is a byte stream with no
//! message boundaries, so every message carries its own length. The kind byte
//! splits control messages from terminal output for the same reason the
//! viewer's WebSocket splits text frames from binary ones: PTY bytes are not
//! text, and routing them through JSON would pay escaping and base64 expansion
//! on the hottest path there is.

use super::protocol::ServerMessage;
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};

/// Largest payload one frame may carry. The reader allocates whatever length
/// the frame announces, so this is the ceiling on what a single message can make
/// the process allocate. Set above the largest payload the protocol actually
/// produces — a terminal scrollback replay, capped at
/// `MAX_TERMINAL_SCROLLBACK_BYTES` (256 KiB) per pane — with room for a control
/// message describing every pane at once.
pub const MAX_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// What a frame carries. Encoded as the first byte on the wire, so an unknown
/// value is a protocol mismatch rather than something to skip: the two sides
/// ship in one binary and cannot legitimately disagree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FrameKind {
    /// A JSON control message.
    Control = 1,
    /// Raw PTY bytes, untouched in either direction.
    Terminal = 2,
}

impl FrameKind {
    fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            1 => Ok(FrameKind::Control),
            2 => Ok(FrameKind::Terminal),
            other => bail!("unknown frame kind {other}"),
        }
    }
}

/// One framed message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn control(payload: Vec<u8>) -> Self {
        Self {
            kind: FrameKind::Control,
            payload,
        }
    }

    pub fn terminal(payload: Vec<u8>) -> Self {
        Self {
            kind: FrameKind::Terminal,
            payload,
        }
    }
}

/// Encode a daemon response as one control frame.
///
/// Serialization is shared here so every producer has the same fallback, while
/// `context` keeps the failing operation visible in the log instead of reducing
/// a session update, reply, and terminal relay to the same diagnostic.
pub(super) fn encode_server(
    message: &ServerMessage,
    context: &'static str,
    fallback_message: &'static str,
) -> Frame {
    match serde_json::to_vec(message) {
        Ok(json) => Frame::control(json),
        Err(err) => {
            tracing::error!(%err, context, "daemon: could not encode a server message");
            let fallback = ServerMessage::Error {
                message: fallback_message.to_string(),
            };
            Frame::control(
                serde_json::to_vec(&fallback).unwrap_or_else(|fallback_err| {
                    tracing::error!(
                        %fallback_err,
                        context,
                        "daemon: could not encode the fallback server error"
                    );
                    br#"{"type":"error","message":"server message could not be encoded"}"#.to_vec()
                }),
            )
        }
    }
}

/// Write one frame: kind byte, big-endian length, payload.
///
/// Does not flush — a caller sending several frames at once should flush after
/// the last, and a caller sending one should flush after it. Flushing here
/// would turn every batch into one syscall per frame.
pub fn write_frame<W: Write>(writer: &mut W, frame: &Frame) -> Result<()> {
    if frame.payload.len() > MAX_FRAME_BYTES {
        bail!(
            "frame payload of {} bytes exceeds the {MAX_FRAME_BYTES}-byte limit",
            frame.payload.len()
        );
    }
    // Built as one buffer and written once: a header written separately can
    // reach the peer as its own packet, and a writer that dies between the two
    // leaves a header with no body for the reader to block on.
    let mut out = Vec::with_capacity(5 + frame.payload.len());
    out.push(frame.kind as u8);
    out.extend_from_slice(&(frame.payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&frame.payload);
    writer.write_all(&out).context("writing a daemon frame")?;
    Ok(())
}

/// Read one frame, or `None` at a clean end of stream.
///
/// `None` means the peer closed between frames, which is how a client detaches;
/// an error means it closed *inside* one, which is a truncated message and not
/// something to resume from.
pub fn read_frame<R: Read>(reader: &mut R) -> Result<Option<Frame>> {
    let mut header = [0u8; 5];
    if !read_exact_or_eof(reader, &mut header)? {
        return Ok(None);
    }
    let kind = FrameKind::from_byte(header[0])?;
    let len = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    // Checked before allocating, not after reading: the length is the one field
    // an untrusted peer controls, and honouring it first is the allocation.
    if len > MAX_FRAME_BYTES {
        bail!("frame announces {len} bytes, over the {MAX_FRAME_BYTES}-byte limit");
    }
    let mut payload = vec![0u8; len];
    if !read_exact_or_eof(reader, &mut payload)? {
        bail!("stream ended inside a frame body of {len} bytes");
    }
    Ok(Some(Frame { kind, payload }))
}

/// Fill `buf`, reporting whether the stream ended before the first byte.
///
/// An end of stream part-way through is an error rather than a `false`: the
/// distinction the caller needs is "nothing more is coming" versus "a message
/// was cut in half", and collapsing them would let a truncated frame look like
/// a clean detach.
fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<bool> {
    if buf.is_empty() {
        return Ok(true);
    }
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => bail!("stream ended after {filled} of {} bytes", buf.len()),
            Ok(n) => filled += n,
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err).context("reading from the daemon socket"),
        }
    }
    Ok(true)
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
