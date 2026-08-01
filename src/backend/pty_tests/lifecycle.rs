use super::*;

#[test]
fn pty_backend_create_and_destroy_pane() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, None).expect("open_pane failed");
    assert_eq!(id, 1);
    backend.destroy_pane(id);
    assert!(!backend.panes.contains_key(&id));
}

#[test]
fn a_pane_whose_shell_exits_reports_it() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, Some("exit")).expect("open_pane");

    assert_eq!(
        drain_until_exit(&mut backend, id).exits,
        1,
        "pane did not report its shell's exit"
    );
}

#[test]
fn a_reported_exit_is_not_reported_again() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, Some("exit")).expect("open_pane");

    assert_eq!(drain_until_exit(&mut backend, id).exits, 1);
    for _ in 0..20 {
        thread::sleep(Duration::from_millis(10));
        for event in backend.drain_events() {
            assert!(
                !matches!(event, BackendEvent::Exited { pane } if pane == id),
                "exit was reported a second time"
            );
        }
    }
    backend.destroy_pane(id);
}

#[test]
#[cfg(unix)]
fn pty_backend_drains_output_before_exit_event() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, None).expect("open_pane failed");
    backend
        .send_input(id, b"printf nightcrow-pty-output; exit\n")
        .expect("send_input failed");

    let drained = drain_until_exit(&mut backend, id);

    assert_eq!(drained.exits, 1, "PTY did not exit before timeout");
    assert!(
        String::from_utf8_lossy(&drained.output).contains("nightcrow-pty-output"),
        "PTY output was not drained before exit"
    );
}

#[test]
#[cfg(unix)]
fn pty_backend_runs_startup_command() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend
        .open_pane(24, 80, Some("printf nightcrow-startup-ran; exit"))
        .expect("open_pane failed");

    let drained = drain_until_exit(&mut backend, id);

    assert!(
        String::from_utf8_lossy(&drained.output).contains("nightcrow-startup-ran"),
        "startup command did not run automatically"
    );
}
