use super::*;
use crate::config::ShellConfig;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

const OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const PTY_IO_DEADLINE: Duration = Duration::from_secs(30);

#[test]
fn real_pty_drains_finite_output_larger_than_its_queue_before_one_exit() {
    let (shell, command) = large_output_command();
    let mut backend = PtyBackend::new(".", shell);
    let pane = backend
        .open_pane(24, 80, Some(&command))
        .expect("open output fixture pane");
    let mut output_bytes = answer_startup_query(&mut backend, pane);
    let deadline = Instant::now() + PTY_IO_DEADLINE;
    let mut exits = 0;

    while exits == 0 && Instant::now() < deadline {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output {
                    pane: event_pane,
                    data,
                } if event_pane == pane => {
                    output_bytes += data.len();
                }
                BackendEvent::Exited { pane: event_pane } if event_pane == pane => exits += 1,
                _ => {}
            }
        }
        thread::yield_now();
    }

    assert_eq!(exits, 1, "PTY should report exactly one exit");
    assert!(
        output_bytes >= OUTPUT_BYTES,
        "expected at least {OUTPUT_BYTES} output bytes, got {output_bytes}"
    );
    backend.destroy_pane(pane);
}

#[test]
fn real_pty_destroy_while_output_channel_is_full_finishes() {
    let (shell, command) = large_output_command();
    let mut backend = PtyBackend::new(".", shell);
    let pane = backend
        .open_pane(24, 80, Some(&command))
        .expect("open output fixture pane");
    answer_startup_query(&mut backend, pane);
    let backpressured = &backend.panes[&pane].output_backpressured;
    let deadline = Instant::now() + PTY_IO_DEADLINE;
    while !backpressured.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::yield_now();
    }
    assert!(
        backpressured.load(Ordering::Acquire),
        "real PTY reader did not block on its full output queue"
    );

    let mut killer = backend.panes[&pane].killer.clone_killer();
    let (done_tx, done_rx) = mpsc::channel();
    let destroyer = thread::spawn(move || {
        backend.destroy_pane(pane);
        let _ = done_tx.send(());
    });
    if done_rx.recv_timeout(Duration::from_secs(5)).is_err() {
        let _ = killer.kill();
        let _ = done_rx.recv_timeout(Duration::from_secs(5));
        panic!("destroying a pane with blocked output did not finish");
    }
    destroyer.join().expect("destroy worker should not panic");
}

fn large_output_command() -> (ShellConfig, String) {
    #[cfg(unix)]
    {
        let blocks = OUTPUT_BYTES / 4096;
        (
            ShellConfig::default(),
            format!("dd if=/dev/zero bs=4096 count={blocks} 2>/dev/null"),
        )
    }
    #[cfg(windows)]
    {
        (
            ShellConfig {
                program: Some("powershell.exe".to_string()),
                command_args: vec!["-NoProfile".to_string(), "-Command".to_string()],
            },
            format!(
                "$bytes = [Text.Encoding]::ASCII.GetBytes(('x' * {OUTPUT_BYTES})); [Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)"
            ),
        )
    }
}

fn answer_startup_query(backend: &mut PtyBackend, pane: PaneId) -> usize {
    #[cfg(windows)]
    {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output_bytes = 0;
        while Instant::now() < deadline {
            for event in backend.drain_events() {
                if let BackendEvent::Output {
                    pane: event_pane,
                    data,
                } = event
                    && event_pane == pane
                {
                    output_bytes += data.len();
                    if data.windows(4).any(|window| window == b"\x1b[6n") {
                        backend
                            .send_input(pane, b"\x1b[1;1R")
                            .expect("answer terminal cursor query");
                        return output_bytes;
                    }
                }
            }
            thread::yield_now();
        }
        output_bytes
    }
    #[cfg(unix)]
    {
        let _ = (backend, pane);
        0
    }
}
