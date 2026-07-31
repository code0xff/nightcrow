use super::*;
use crate::backend::identity::FIRST_GENERATION;
use crate::config::ShellConfig;
use std::time::{Duration, Instant};

/// Long enough that a `printf` and its exit are certainly drained.
const RELAUNCH_MARKER: &str = "nightcrow-relaunched";

#[test]
fn pty_backend_create_and_destroy_pane() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, None).expect("open_pane failed");
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
#[cfg(unix)]
fn pty_backend_drains_output_before_exit_event() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, None).expect("open_pane failed");

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
                // A local backend answers `open_pane` directly and its panes
                // are nobody else's to size, so none of the rest occur here.
                _ => {}
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
fn opening_a_pane_gives_it_a_token_at_the_first_generation() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, None).expect("open_pane failed");

    let identity = backend.slot(id).expect("pane has a slot").identity.clone();
    assert_eq!(identity.generation, FIRST_GENERATION);
    assert!(!identity.token.as_str().is_empty());

    backend.destroy_pane(id);
}

#[test]
fn a_token_resolves_to_the_pane_holding_it() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, None).expect("open_pane failed");
    let token = backend.slot(id).expect("slot").identity.token.clone();

    assert_eq!(backend.pane_for_token(&token), Some(id));
    backend.destroy_pane(id);
    assert_eq!(backend.pane_for_token(&token), None);
}

#[test]
fn relaunching_a_pane_keeps_the_token_and_advances_the_generation() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let first = backend
        .open_pane(24, 80, Some("printf first; exit"))
        .expect("open_pane failed");
    let token = backend.slot(first).expect("slot").identity.token.clone();

    let second = backend
        .relaunch_pane(first, 24, 80, &[], &[])
        .expect("relaunch failed");

    // A new id is unavoidable — ids are monotonic and clients treat `Exited` as
    // final — so the token is what carries the slot's identity across.
    assert_ne!(second, first);
    let slot = backend.slot(second).expect("relaunched slot");
    assert_eq!(slot.identity.token, token);
    assert_eq!(slot.identity.generation, FIRST_GENERATION + 1);
    // The old id stops resolving, so a decision made about it cannot land here.
    assert!(backend.slot(first).is_none());
    assert_eq!(backend.pane_for_token(&token), Some(second));
}

#[test]
#[cfg(unix)]
fn a_relaunch_reproduces_the_original_command() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let first = backend
        .open_pane(24, 80, Some(&format!("printf {RELAUNCH_MARKER}; exit")))
        .expect("open_pane failed");
    let second = backend
        .relaunch_pane(first, 24, 80, &[], &[])
        .expect("relaunch failed");

    let deadline = Instant::now() + PTY_TEST_DEADLINE;
    let mut output = Vec::new();
    let mut saw_exit = false;
    while Instant::now() < deadline {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { pane, data } if pane == second => output.extend(data),
                BackendEvent::Exited { pane } if pane == second => saw_exit = true,
                _ => {}
            }
        }
        if saw_exit {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        String::from_utf8_lossy(&output).contains(RELAUNCH_MARKER),
        "relaunch did not re-run the original command"
    );
}

#[test]
fn a_relaunch_keeps_the_original_command_rather_than_accumulating_resume_args() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let allowed = vec!["--flag".to_string()];
    let args = vec!["--flag".to_string()];
    let first = backend
        .open_pane(24, 80, Some("true"))
        .expect("open_pane failed");

    let second = backend
        .relaunch_pane(first, 24, 80, &args, &allowed)
        .expect("first relaunch");
    // The retained launch is the original invocation, so a second relaunch does
    // not stack another copy of the resume arguments onto it.
    assert_eq!(
        backend
            .slot(second)
            .expect("slot")
            .launch
            .command
            .as_deref(),
        Some("true")
    );

    let third = backend
        .relaunch_pane(second, 24, 80, &args, &allowed)
        .expect("second relaunch");
    assert_eq!(
        backend.slot(third).expect("slot").launch.command.as_deref(),
        Some("true")
    );
}

#[test]
fn relaunching_a_pane_that_is_gone_is_refused() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, Some("true")).expect("open_pane");
    backend.destroy_pane(id);

    let err = backend.relaunch_pane(id, 24, 80, &[], &[]).unwrap_err();
    assert!(err.to_string().contains("no slot"), "{err}");
}

#[test]
fn a_refused_relaunch_leaves_the_pane_running() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend
        .open_pane(24, 80, Some("sleep 30"))
        .expect("open_pane");
    let token = backend.slot(id).expect("slot").identity.token.clone();

    // Not in the allowlist, so the command line is refused before anything is
    // torn down.
    let args = vec!["--nope".to_string()];
    assert!(backend.relaunch_pane(id, 24, 80, &args, &[]).is_err());

    assert_eq!(backend.pane_for_token(&token), Some(id));
    assert!(backend.panes.contains_key(&id));
    backend.destroy_pane(id);
}

#[test]
fn two_panes_get_distinct_tokens() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let a = backend.open_pane(24, 80, None).expect("open_pane failed");
    let b = backend.open_pane(24, 80, None).expect("open_pane failed");

    // Two panes on one repository is a supported layout, so the token — not the
    // working directory — is what tells them apart.
    let ta = backend.slot(a).expect("a").identity.token.clone();
    let tb = backend.slot(b).expect("b").identity.token.clone();
    assert_ne!(ta, tb);

    backend.destroy_pane(a);
    backend.destroy_pane(b);
}

#[test]
fn destroying_a_pane_retires_its_token() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend.open_pane(24, 80, None).expect("open_pane failed");
    backend.destroy_pane(id);

    // A held token must stop resolving, or it would address whatever id lands
    // in this slot next.
    assert!(backend.slot(id).is_none());
}

#[test]
#[cfg(unix)]
fn a_panes_child_process_sees_its_token_in_the_environment() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let id = backend
        .open_pane(24, 80, Some("printf %s \"$NIGHTCROW_PANE_TOKEN\"; exit"))
        .expect("open_pane failed");
    let token = backend.slot(id).expect("slot").identity.token.clone();

    let deadline = Instant::now() + PTY_TEST_DEADLINE;
    let mut output = Vec::new();
    let mut saw_exit = false;
    while Instant::now() < deadline {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { data, .. } => output.extend(data),
                BackendEvent::Exited { pane } if pane == id => saw_exit = true,
                _ => {}
            }
        }
        if saw_exit {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    // This is the correlation path a provider's own hook processes inherit.
    assert!(
        String::from_utf8_lossy(&output).contains(token.as_str()),
        "pane token was not exported to the child environment"
    );
}

#[test]
#[cfg(unix)]
fn pty_backend_runs_startup_command() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    // The command runs itself on launch — no input is sent. `exit` keeps
    // the test bounded by ending the shell after the command prints.
    let id = backend
        .open_pane(24, 80, Some("printf nightcrow-startup-ran; exit"))
        .expect("open_pane failed");

    let deadline = Instant::now() + PTY_TEST_DEADLINE;
    let mut output = Vec::new();
    let mut saw_exit = false;
    while Instant::now() < deadline {
        for event in backend.drain_events() {
            match event {
                BackendEvent::Output { data, .. } => output.extend(data),
                BackendEvent::Exited { pane } if pane == id => saw_exit = true,
                // A local backend answers `open_pane` directly and its panes
                // are nobody else's to size, so none of the rest occur here.
                _ => {}
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
