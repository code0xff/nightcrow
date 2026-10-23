use crate::backend::pty::PtyEvent;
use crate::backend::pty::io::{
    INPUT_BYTE_LIMIT, INPUT_MESSAGE_LIMIT, InputQueue, spawn_output_reader,
};
use std::io::{self, Cursor, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const IO_TEST_DEADLINE: Duration = Duration::from_secs(2);

struct BlockingWriter {
    started: Option<mpsc::Sender<()>>,
    release: Receiver<()>,
}

impl Write for BlockingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Some(started) = self.started.take() {
            let _ = started.send(());
            let _ = self.release.recv();
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct GatedFailWriter {
    started: Option<mpsc::Sender<()>>,
    release: Receiver<()>,
}

impl Write for GatedFailWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        if let Some(started) = self.started.take() {
            let _ = started.send(());
            let _ = self.release.recv();
        }
        Err(io::Error::other("fixture write failed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn input_submission_returns_while_the_writer_is_blocked() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (queue, worker) = InputQueue::spawn(BlockingWriter {
        started: Some(started_tx),
        release: release_rx,
    });
    queue.enqueue(b"first").expect("enqueue first input");
    started_rx
        .recv_timeout(IO_TEST_DEADLINE)
        .expect("writer should start and block");

    let queued = Arc::new(queue);
    let worker_queue = Arc::clone(&queued);
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        done_tx
            .send(worker_queue.enqueue(b"second"))
            .expect("send result")
    });
    let result = done_rx.recv_timeout(IO_TEST_DEADLINE);
    let _ = release_tx.send(());
    drop(queued);
    assert!(
        result
            .expect("enqueue must not wait for the PTY writer")
            .is_ok()
    );
    crate::platform::threading::try_timed_join(worker, IO_TEST_DEADLINE);
}

#[test]
fn input_byte_and_message_limits_include_the_in_flight_write() {
    let (queue, worker, release) = blocked_input_queue();
    queue
        .enqueue(&vec![0; INPUT_BYTE_LIMIT - b"in flight".len()])
        .expect("remaining byte budget fits");
    assert!(
        queue.enqueue(b"x").is_err(),
        "byte limit must include inflight"
    );
    drop(queue);
    let _ = release.send(());
    crate::platform::threading::try_timed_join(worker, IO_TEST_DEADLINE);

    let (queue, worker, release) = blocked_input_queue();
    for _ in 1..INPUT_MESSAGE_LIMIT {
        queue.enqueue(b"x").expect("message slot should fit");
    }
    assert!(
        queue.enqueue(b"overflow").is_err(),
        "message limit must include inflight"
    );
    drop(queue);
    let _ = release.send(());
    crate::platform::threading::try_timed_join(worker, IO_TEST_DEADLINE);
}

#[test]
fn oversized_input_is_rejected_atomically() {
    let (queue, worker, release) = blocked_input_queue();
    assert!(queue.enqueue(&vec![0; INPUT_BYTE_LIMIT + 1]).is_err());
    queue
        .enqueue(b"still fits")
        .expect("oversize input was not partly queued");
    drop(queue);
    let _ = release.send(());
    crate::platform::threading::try_timed_join(worker, IO_TEST_DEADLINE);
}

#[test]
fn input_writer_failure_rejects_later_submissions() {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (queue, worker) = InputQueue::spawn(GatedFailWriter {
        started: Some(started_tx),
        release: release_rx,
    });
    queue.enqueue(b"first").expect("enqueue first input");
    started_rx
        .recv_timeout(IO_TEST_DEADLINE)
        .expect("writer should start and block");
    let _ = release_tx.send(());
    assert!(queue.wait_for_failure(IO_TEST_DEADLINE));
    assert!(queue.enqueue(b"later").is_err());
    drop(queue);
    crate::platform::threading::try_timed_join(worker, IO_TEST_DEADLINE);
}

#[test]
fn output_reader_preserves_fifo_with_a_bounded_channel() {
    let expected: Vec<u8> = (0..(3 * 4096 + 17)).map(|index| index as u8).collect();
    let (tx, rx) = mpsc::sync_channel(1);
    let reader = spawn_output_reader(Cursor::new(expected.clone()), tx);
    let mut actual = Vec::new();
    loop {
        match rx.recv_timeout(IO_TEST_DEADLINE).expect("reader event") {
            PtyEvent::Output(data) => actual.extend(data),
            PtyEvent::Exited => break,
            PtyEvent::ChildExited => panic!("reader does not report child exit"),
        }
    }
    crate::platform::threading::try_timed_join(reader, IO_TEST_DEADLINE);
    assert_eq!(actual, expected);
}

#[test]
fn dropping_a_full_output_receiver_unblocks_sender_and_drains_reader() {
    let payload = vec![42; 4 * 4096 + 3];
    let bytes_read = Arc::new(AtomicUsize::new(0));
    let (read_started_tx, read_started_rx) = mpsc::channel();
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(PtyEvent::ChildExited).expect("fill output queue");
    let reader = spawn_output_reader(
        CountingReader {
            inner: Cursor::new(payload.clone()),
            bytes_read: Arc::clone(&bytes_read),
            started: Some(read_started_tx),
        },
        tx,
    );
    read_started_rx
        .recv_timeout(IO_TEST_DEADLINE)
        .expect("reader should reach full queue");
    drop(rx);
    crate::platform::threading::try_timed_join(reader, IO_TEST_DEADLINE);
    assert_eq!(bytes_read.load(Ordering::SeqCst), payload.len());
}

fn blocked_input_queue() -> (InputQueue, thread::JoinHandle<()>, mpsc::Sender<()>) {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (queue, worker) = InputQueue::spawn(BlockingWriter {
        started: Some(started_tx),
        release: release_rx,
    });
    queue
        .enqueue(b"in flight")
        .expect("enqueue in-flight input");
    started_rx
        .recv_timeout(IO_TEST_DEADLINE)
        .expect("writer should start and block");
    (queue, worker, release_tx)
}

struct CountingReader {
    inner: Cursor<Vec<u8>>,
    bytes_read: Arc<AtomicUsize>,
    started: Option<mpsc::Sender<()>>,
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if let Some(started) = self.started.take() {
            let _ = started.send(());
        }
        let read = self.inner.read(buffer)?;
        self.bytes_read.fetch_add(read, Ordering::SeqCst);
        Ok(read)
    }
}
