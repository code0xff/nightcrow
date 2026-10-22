use super::PtyEvent;
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::sync::{Arc, Condvar, Mutex, mpsc::SyncSender};
use std::thread::{self, JoinHandle};

pub(super) const OUTPUT_EVENT_CAPACITY: usize = 256;
pub(super) const IO_CHUNK_BYTES: usize = 4096;
pub(super) const INPUT_BYTE_LIMIT: usize = 4 * 1024 * 1024;
pub(super) const INPUT_MESSAGE_LIMIT: usize = 256;

struct InputState {
    queue: VecDeque<Vec<u8>>,
    pending_bytes: usize,
    pending_messages: usize,
    closed: bool,
    failure: Option<String>,
}

struct InputShared {
    state: Mutex<InputState>,
    ready: Condvar,
}

pub(super) struct InputQueue {
    shared: Arc<InputShared>,
}

impl InputQueue {
    pub(super) fn spawn<W: Write + Send + 'static>(writer: W) -> (Self, JoinHandle<()>) {
        let shared = Arc::new(InputShared {
            state: Mutex::new(InputState {
                queue: VecDeque::new(),
                pending_bytes: 0,
                pending_messages: 0,
                closed: false,
                failure: None,
            }),
            ready: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let handle = thread::spawn(move || write_inputs(writer, worker_shared));
        (Self { shared }, handle)
    }

    pub(super) fn enqueue(&self, data: &[u8]) -> anyhow::Result<()> {
        let mut state = lock(&self.shared.state);
        if let Some(error) = &state.failure {
            anyhow::bail!("PTY input writer failed: {error}");
        }
        if state.closed {
            anyhow::bail!("PTY input queue is closed");
        }
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > INPUT_BYTE_LIMIT
            || state.pending_messages >= INPUT_MESSAGE_LIMIT
            || state.pending_bytes.saturating_add(data.len()) > INPUT_BYTE_LIMIT
        {
            anyhow::bail!("PTY input queue is full");
        }
        state.queue.push_back(data.to_vec());
        state.pending_bytes += data.len();
        state.pending_messages += 1;
        self.shared.ready.notify_one();
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn wait_for_failure(&self, timeout: std::time::Duration) -> bool {
        let state = lock(&self.shared.state);
        let (state, _) = self
            .shared
            .ready
            .wait_timeout_while(state, timeout, |state| state.failure.is_none())
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.failure.is_some()
    }

    fn close(&self) {
        let mut state = lock(&self.shared.state);
        state.closed = true;
        let queued_bytes: usize = state.queue.iter().map(Vec::len).sum();
        state.pending_bytes -= queued_bytes;
        state.pending_messages -= state.queue.len();
        state.queue.clear();
        self.shared.ready.notify_all();
    }
}

impl Drop for InputQueue {
    fn drop(&mut self) {
        self.close();
    }
}

fn write_inputs<W: Write>(mut writer: W, shared: Arc<InputShared>) {
    loop {
        let data = {
            let mut state = lock(&shared.state);
            while state.queue.is_empty() && !state.closed && state.failure.is_none() {
                state = shared
                    .ready
                    .wait(state)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
            if state.closed || state.failure.is_some() {
                return;
            }
            state.queue.pop_front().expect("queue checked above")
        };

        let result = writer.write_all(&data).and_then(|()| writer.flush());
        let mut state = lock(&shared.state);
        state.pending_bytes -= data.len();
        state.pending_messages -= 1;
        if let Err(error) = result {
            tracing::warn!(%error, "PTY input writer failed");
            let queued_bytes: usize = state.queue.iter().map(Vec::len).sum();
            state.pending_bytes -= queued_bytes;
            state.pending_messages -= state.queue.len();
            state.queue.clear();
            state.failure = Some(error.to_string());
            shared.ready.notify_all();
            return;
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(super) fn spawn_output_reader<R: Read + Send + 'static>(
    reader: R,
    tx: SyncSender<PtyEvent>,
) -> JoinHandle<()> {
    spawn_output_reader_inner(reader, tx, |_| {})
}

#[cfg(test)]
pub(super) fn spawn_output_reader_observed<R: Read + Send + 'static>(
    reader: R,
    tx: SyncSender<PtyEvent>,
    backpressured: Arc<std::sync::atomic::AtomicBool>,
) -> JoinHandle<()> {
    spawn_output_reader_inner(reader, tx, move |blocked| {
        backpressured.store(blocked, std::sync::atomic::Ordering::Release);
    })
}

fn spawn_output_reader_inner<R, F>(
    mut reader: R,
    tx: SyncSender<PtyEvent>,
    mut set_backpressure: F,
) -> JoinHandle<()>
where
    R: Read + Send + 'static,
    F: FnMut(bool) + Send + 'static,
{
    thread::spawn(move || {
        let mut connected = true;
        let mut buf = [0u8; IO_CHUNK_BYTES];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) if connected => match tx.try_send(PtyEvent::Output(buf[..n].to_vec())) {
                    Ok(()) => {}
                    Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                        connected = false;
                    }
                    Err(std::sync::mpsc::TrySendError::Full(event)) => {
                        set_backpressure(true);
                        let sent = tx.send(event).is_ok();
                        set_backpressure(false);
                        connected = sent;
                    }
                },
                Ok(_) => {}
            }
        }
        if connected {
            let _ = tx.send(PtyEvent::Exited);
        }
    })
}
