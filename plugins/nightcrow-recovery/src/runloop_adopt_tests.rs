//! Asking for a pane: what is asked once, what is not asked twice, and what a
//! stream of tokens we will never be given can cost this process.

use super::*;
use crate::provider::SignalKind;

fn signal(token: &str) -> IpcMessage {
    IpcMessage {
        token: token.to_string(),
        kind: SignalKind::StopFailure,
        payload: serde_json::json!({"error_type": "rate_limit"}),
    }
}

fn asked_token(command: &PluginCommand) -> String {
    match command {
        PluginCommand::WatchPane { token, .. } => token.clone(),
        other => panic!("expected a watch_pane request, got {other:?}"),
    }
}

#[test]
fn a_signal_for_an_unknown_pane_asks_the_host_for_it_by_token() {
    let mut a = Adoptions::default();
    let command = a
        .request(signal("abc123"), Instant::now())
        .expect("a first signal asks");
    assert_eq!(asked_token(&command), "abc123");
}

#[test]
fn a_second_signal_for_the_same_token_asks_nothing_more() {
    // The host answers in milliseconds or never, so a repeat inside the cooldown
    // could only be noise — and Claude Code's statusline is noisy: it runs on
    // every render.
    let mut a = Adoptions::default();
    let now = Instant::now();
    assert!(a.request(signal("abc123"), now).is_some());
    for _ in 0..50 {
        assert!(a.request(signal("abc123"), now).is_none());
    }
}

#[test]
fn a_token_may_be_asked_about_again_once_its_request_has_been_given_up_on() {
    let mut a = Adoptions::default();
    let now = Instant::now();
    assert!(a.request(signal("abc123"), now).is_some());

    a.prune(now + REQUEST_COOLDOWN);

    assert!(
        a.request(signal("abc123"), now + REQUEST_COOLDOWN)
            .is_some(),
        "a pane that only just became ours must not be shut out for good"
    );
}

#[test]
fn a_request_still_inside_its_cooldown_survives_pruning() {
    let mut a = Adoptions::default();
    let now = Instant::now();
    assert!(a.request(signal("abc123"), now).is_some());

    a.prune(now + REQUEST_COOLDOWN / 2);

    assert!(
        a.request(signal("abc123"), now + REQUEST_COOLDOWN / 2)
            .is_none()
    );
}

#[test]
fn a_flood_of_unknown_tokens_stops_at_the_pending_ceiling() {
    // Unsolicited state: anything that can reach the socket can name a token we
    // have never seen, so this must stop growing rather than stop working.
    let mut a = Adoptions::default();
    let now = Instant::now();
    for i in 0..MAX_PENDING {
        assert!(
            a.request(signal(&format!("token{i}")), now).is_some(),
            "the first {MAX_PENDING} tokens are asked about"
        );
    }
    assert!(
        a.request(signal("onemore"), now).is_none(),
        "past the ceiling a new token is dropped, not queued"
    );
    // And the dropped token left nothing behind, so the ceiling is a ceiling on
    // memory and not merely on requests.
    a.prune(now + REQUEST_COOLDOWN);
    assert!(
        a.request(signal("onemore"), now + REQUEST_COOLDOWN)
            .is_some()
    );
}

#[test]
fn the_signal_that_won_a_pane_its_request_is_handed_back_when_the_pane_arrives() {
    // The signal arrives before the pane does and the host replays no history, so
    // losing it here would lose the very limit the recovery is about.
    let mut a = Adoptions::default();
    assert!(a.request(signal("abc123"), Instant::now()).is_some());

    let held = a.claim("abc123").expect("the signal was kept");

    assert_eq!(held.kind, SignalKind::StopFailure);
    assert_eq!(held.payload["error_type"], "rate_limit");
}

#[test]
fn a_claimed_request_is_not_handed_back_a_second_time() {
    let mut a = Adoptions::default();
    assert!(a.request(signal("abc123"), Instant::now()).is_some());
    assert!(a.claim("abc123").is_some());
    assert!(
        a.claim("abc123").is_none(),
        "one signal must not be applied twice"
    );
}

#[test]
fn a_pane_we_never_asked_about_has_no_signal_waiting_for_it() {
    // Every configured pane takes this path: the host named it, so no request was
    // ever made for it.
    let mut a = Adoptions::default();
    assert!(a.claim("never-asked").is_none());
}
