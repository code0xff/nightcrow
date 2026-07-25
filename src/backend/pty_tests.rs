use super::*;
use std::time::{Duration, Instant};

#[test]
fn pty_backend_create_and_destroy_pane() {
    let mut backend = PtyBackend::new(".");
    let id = backend
        .create_pane(24, 80, None)
        .expect("create_pane failed");
    assert_eq!(id, 1);
    backend.destroy_pane(id);
    assert!(!backend.panes.contains_key(&id));
}

/// Deadline for the real-PTY tests below. They spawn the user's actual
/// `$SHELL` (an interactive zsh sources the full rc chain), and cargo
/// runs tests in parallel, so several shells can be initializing at
/// once — under load a 3 s budget was measurably flaky (~2/25 runs).
/// A generous bound only delays the failure verdict; passing runs
/// still finish as soon as the events arrive.
const PTY_TEST_DEADLINE: Duration = Duration::from_secs(15);

#[test]
fn pty_backend_drains_output_before_exit_event() {
    let mut backend = PtyBackend::new(".");
    let id = backend
        .create_pane(24, 80, None)
        .expect("create_pane failed");

    backend
        .send_input(id, b"printf nightcrow-pty-output; exit\n")
        .expect("send_input failed");

    let deadline = Instant::now() + PTY_TEST_DEADLINE;
    let mut output = Vec::new();
    let mut saw_exit = false;
    while Instant::now() < deadline {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { data, .. } => output.extend(data),
                BackendEvent::Exited { pane } if pane == id => saw_exit = true,
                BackendEvent::Exited { .. } => {}
            }
        }
        if saw_exit {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(saw_exit, "PTY did not exit before timeout");
    assert!(
        String::from_utf8_lossy(&output).contains("nightcrow-pty-output"),
        "PTY output was not drained before exit"
    );
}

#[test]
fn pty_backend_runs_startup_command() {
    let mut backend = PtyBackend::new(".");
    // The command runs itself on launch — no input is sent. `exit` keeps
    // the test bounded by ending the shell after the command prints.
    let id = backend
        .create_pane(24, 80, Some("printf nightcrow-startup-ran; exit"))
        .expect("create_pane failed");

    let deadline = Instant::now() + PTY_TEST_DEADLINE;
    let mut output = Vec::new();
    let mut saw_exit = false;
    while Instant::now() < deadline {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { data, .. } => output.extend(data),
                BackendEvent::Exited { pane } if pane == id => saw_exit = true,
                BackendEvent::Exited { .. } => {}
            }
        }
        if saw_exit {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        String::from_utf8_lossy(&output).contains("nightcrow-startup-ran"),
        "startup command did not run automatically"
    );
}
