use super::*;

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

    assert_ne!(second, first);
    let slot = backend.slot(second).expect("relaunched slot");
    assert_eq!(slot.identity.token, token);
    assert_eq!(slot.identity.generation, FIRST_GENERATION + 1);
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

    let drained = drain_until_exit(&mut backend, second);

    assert!(
        String::from_utf8_lossy(&drained.output).contains(RELAUNCH_MARKER),
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

    let args = vec!["--nope".to_string()];
    assert!(backend.relaunch_pane(id, 24, 80, &args, &[]).is_err());
    assert_eq!(backend.pane_for_token(&token), Some(id));
    assert!(backend.panes.contains_key(&id));
    backend.destroy_pane(id);
}
