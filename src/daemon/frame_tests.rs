use super::{Frame, FrameKind, MAX_FRAME_BYTES, read_frame, write_frame};

/// Round-trip `frames` through one buffer, as a socket would carry them.
fn round_trip(frames: &[Frame]) -> Vec<Frame> {
    let mut wire = Vec::new();
    for frame in frames {
        write_frame(&mut wire, frame).expect("writing fits the limit");
    }
    let mut reader = wire.as_slice();
    let mut out = Vec::new();
    while let Some(frame) = read_frame(&mut reader).expect("the stream is well formed") {
        out.push(frame);
    }
    out
}

#[test]
fn a_frame_survives_the_round_trip() {
    let frames = vec![
        Frame::control(br#"{"type":"hello"}"#.to_vec()),
        Frame::terminal(vec![0x1b, b'[', b'2', b'J']),
    ];
    assert_eq!(round_trip(&frames), frames);
}

#[test]
fn back_to_back_frames_do_not_bleed_into_each_other() {
    // The point of framing: a stream carries no boundaries, so two messages
    // written together must come back as two, not one concatenated blob.
    let frames: Vec<Frame> = (0..50)
        .map(|i| Frame::terminal(vec![i as u8; i as usize]))
        .collect();
    assert_eq!(round_trip(&frames), frames);
}

#[test]
fn an_empty_payload_is_a_frame_not_an_end_of_stream() {
    // A control message can be a bare tag with no body. Treating its zero
    // length as EOF would drop it and close the connection.
    let frames = vec![Frame::control(Vec::new()), Frame::terminal(vec![7])];
    assert_eq!(round_trip(&frames), frames);
}

#[test]
fn raw_terminal_bytes_pass_through_unchanged() {
    // PTY output is not text. Every byte value must survive, including NUL and
    // sequences that are not valid UTF-8.
    let payload: Vec<u8> = (0..=255u8).collect();
    let frames = vec![Frame::terminal(payload.clone())];
    assert_eq!(round_trip(&frames)[0].payload, payload);
}

#[test]
fn a_closed_stream_between_frames_reads_as_the_end() {
    let mut empty: &[u8] = &[];
    assert!(
        read_frame(&mut empty)
            .expect("a clean close is not an error")
            .is_none()
    );
}

#[test]
fn a_stream_that_ends_inside_a_header_is_an_error() {
    let mut partial: &[u8] = &[1, 0, 0];
    assert!(
        read_frame(&mut partial).is_err(),
        "a half-written header is a truncated message, not a clean close"
    );
}

#[test]
fn a_stream_that_ends_inside_a_body_is_an_error() {
    let mut wire = Vec::new();
    write_frame(&mut wire, &Frame::control(vec![1, 2, 3, 4])).unwrap();
    wire.truncate(wire.len() - 2);

    let mut reader = wire.as_slice();
    assert!(
        read_frame(&mut reader).is_err(),
        "a body cut short must not be reported as a shorter message"
    );
}

#[test]
fn an_unknown_kind_is_rejected() {
    // Both sides ship in one binary, so an unrecognized kind means the stream
    // is not what it claims to be rather than a newer peer to tolerate.
    let mut wire: &[u8] = &[9, 0, 0, 0, 0];
    assert!(read_frame(&mut wire).is_err());
}

#[test]
fn an_oversized_length_is_refused_before_it_is_allocated() {
    // The announced length is the one field a peer controls, and the reader
    // allocates it. u32::MAX here would be a 4 GiB allocation on trust.
    let mut wire: &[u8] = &[1, 0xff, 0xff, 0xff, 0xff];
    let err = read_frame(&mut wire).expect_err("an absurd length is refused");
    assert!(
        err.to_string().contains("limit"),
        "the error should name the limit: {err}"
    );
}

#[test]
fn a_payload_over_the_limit_is_refused_at_the_writer_too() {
    // Caught on the way out as well, so a bug on this side surfaces here
    // rather than as an unreadable frame at the peer.
    let frame = Frame::terminal(vec![0u8; MAX_FRAME_BYTES + 1]);
    let mut wire = Vec::new();
    assert!(write_frame(&mut wire, &frame).is_err());
    assert!(wire.is_empty(), "a refused frame must not write a header");
}

#[test]
fn a_payload_at_exactly_the_limit_is_allowed() {
    let frame = Frame::terminal(vec![0u8; MAX_FRAME_BYTES]);
    let mut wire = Vec::new();
    write_frame(&mut wire, &frame).expect("the limit itself is not over it");
    let mut reader = wire.as_slice();
    let read = read_frame(&mut reader).unwrap().expect("a frame");
    assert_eq!(read.kind, FrameKind::Terminal);
    assert_eq!(read.payload.len(), MAX_FRAME_BYTES);
}

#[test]
fn a_short_read_is_resumed_rather_than_treated_as_the_end() {
    // Sockets return partial reads. A reader that took the first `read` as the
    // whole frame would corrupt every message split across packets.
    struct Dribble<'a> {
        data: &'a [u8],
    }
    impl std::io::Read for Dribble<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.data.is_empty() {
                return Ok(0);
            }
            // One byte per call, the worst case a socket can hand back.
            buf[0] = self.data[0];
            self.data = &self.data[1..];
            Ok(1)
        }
    }

    let frame = Frame::control(b"a message split across many reads".to_vec());
    let mut wire = Vec::new();
    write_frame(&mut wire, &frame).unwrap();

    let mut reader = Dribble { data: &wire };
    assert_eq!(read_frame(&mut reader).unwrap().expect("a frame"), frame);
}
