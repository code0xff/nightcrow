use super::*;

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
fn two_panes_get_distinct_tokens() {
    let mut backend = PtyBackend::new(".", ShellConfig::default());
    let a = backend.open_pane(24, 80, None).expect("open_pane failed");
    let b = backend.open_pane(24, 80, None).expect("open_pane failed");

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

    let drained = drain_until_exit(&mut backend, id);

    assert!(
        String::from_utf8_lossy(&drained.output).contains(token.as_str()),
        "pane token was not exported to the child environment"
    );
}
