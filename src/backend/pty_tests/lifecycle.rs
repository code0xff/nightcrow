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
fn destroying_a_pane_terminates_its_subprocess() {
    let dir = tempfile::tempdir().expect("tempdir");
    let marker = dir.path().join("child.pid");
    let (shell, command) = descendant_command(&marker);
    let mut backend = PtyBackend::new(".", shell);
    let id = backend
        .open_pane(24, 80, Some(&command))
        .expect("open pane with descendant");
    let pid = wait_for_pid(&mut backend, id, &marker);

    backend.destroy_pane(id);

    let deadline = Instant::now() + Duration::from_secs(5);
    while process_is_alive(pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(
        !process_is_alive(pid),
        "pane descendant {pid} survived close"
    );
}

#[cfg(unix)]
fn descendant_command(marker: &std::path::Path) -> (ShellConfig, String) {
    (
        ShellConfig::default(),
        format!("sh -c 'echo $$ > \"{}\"; exec sleep 60'", marker.display()),
    )
}

#[cfg(windows)]
fn descendant_command(marker: &std::path::Path) -> (ShellConfig, String) {
    let shell = ShellConfig {
        program: Some("powershell.exe".to_string()),
        command_args: vec!["-NoProfile".to_string(), "-Command".to_string()],
    };
    let command = format!(
        "$p = Start-Process cmd.exe -WindowStyle Hidden -ArgumentList '/C','ping -t 127.0.0.1' -PassThru; [IO.File]::WriteAllText('{}', [string]$p.Id); $p.WaitForExit()",
        marker.display()
    );
    (shell, command)
}

fn wait_for_pid(backend: &mut PtyBackend, id: PaneId, marker: &std::path::Path) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut output = Vec::new();
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(marker)
            && let Ok(pid) = text.trim().parse()
        {
            return pid;
        }
        for event in backend.drain_events() {
            if let BackendEvent::Output { pane, data } = event
                && pane == id
            {
                if data.windows(4).any(|window| window == b"\x1b[6n") {
                    let _ = backend.send_input(id, b"\x1b[1;1R");
                }
                output.extend(data);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!(
        "subprocess did not write {}: {}",
        marker.display(),
        String::from_utf8_lossy(&output)
    );
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn process_is_alive(pid: u32) -> bool {
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        && stat.rfind(')').and_then(|end| stat.as_bytes().get(end + 2)) == Some(&b'Z')
    {
        return false;
    }
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
fn process_is_alive(pid: u32) -> bool {
    let result = unsafe { libc::kill(pid as libc::pid_t, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn process_is_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    const SYNCHRONIZE_ACCESS: u32 = 0x0010_0000;
    let process = unsafe { OpenProcess(SYNCHRONIZE_ACCESS, 0, pid) };
    if process.is_null() {
        return false;
    }
    let alive = unsafe { WaitForSingleObject(process, 0) } == WAIT_TIMEOUT;
    unsafe { CloseHandle(process) };
    alive
}

#[test]
fn resizing_an_unknown_pane_is_reported() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());

    let error = backend.resize(999, 24, 80).expect_err("unknown pane");

    assert!(error.to_string().contains("pane 999 not found"));
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
