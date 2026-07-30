use super::*;
use crate::backend::identity::FIRST_GENERATION;

fn launch(command: &str) -> PaneLaunch {
    PaneLaunch {
        command: Some(command.to_string()),
    }
}

fn slots_with_one(now: Instant) -> (PaneSlots, PaneId, PaneToken) {
    let mut slots = PaneSlots::default();
    let identity = PaneIdentity::new().expect("OS RNG");
    let token = identity.token.clone();
    slots.insert(7, identity, launch("agent"), now);
    (slots, 7, token)
}

#[test]
fn a_freshly_opened_pane_counts_as_quiet_since_it_opened() {
    let now = Instant::now();
    let (slots, id, _) = slots_with_one(now);
    // Otherwise a pane that has printed nothing could never be called idle.
    assert_eq!(slots.get(id).expect("slot").idle_for(now), Duration::ZERO);
}

#[test]
fn marking_output_restarts_the_idle_clock() {
    let start = Instant::now();
    let (mut slots, id, _) = slots_with_one(start);
    let later = start + Duration::from_secs(5);
    slots.mark_output(id, later);

    assert_eq!(slots.get(id).expect("slot").idle_for(later), Duration::ZERO);
    assert_eq!(
        slots
            .get(id)
            .expect("slot")
            .idle_for(later + Duration::from_secs(2)),
        Duration::from_secs(2)
    );
}

#[test]
fn marking_output_on_a_pane_that_is_gone_is_ignored() {
    let now = Instant::now();
    let (mut slots, id, _) = slots_with_one(now);
    slots.remove(id);
    slots.mark_output(id, now);
    assert!(slots.get(id).is_none());
}

#[test]
fn a_token_resolves_to_its_pane_and_stops_resolving_once_removed() {
    let now = Instant::now();
    let (mut slots, id, token) = slots_with_one(now);
    assert_eq!(slots.find_by_token(&token), Some(id));

    slots.remove(id);
    // A held token must not address whatever occupies the slot next.
    assert_eq!(slots.find_by_token(&token), None);
}

#[test]
fn a_slot_keeps_the_launch_command_so_it_can_be_reproduced() {
    let now = Instant::now();
    let (slots, id, _) = slots_with_one(now);
    assert_eq!(
        slots.get(id).expect("slot").launch.command.as_deref(),
        Some("agent")
    );
    assert_eq!(
        slots.get(id).expect("slot").identity.generation,
        FIRST_GENERATION
    );
}

#[test]
fn no_resume_arguments_leaves_the_original_command_untouched() {
    let line = resume_command_line(Some("agent --model x"), &[], &[]).expect("no args is fine");
    assert_eq!(line, "agent --model x");
}

#[test]
fn an_allowed_flag_and_its_value_are_appended_quoted() {
    let args = vec!["--resume".to_string(), "abc-123".to_string()];
    let allowed = vec!["--resume".to_string()];
    let line = resume_command_line(Some("agent"), &args, &allowed).expect("allowed");
    assert_eq!(line, "agent '--resume' 'abc-123'");
}

#[test]
fn a_flag_the_plugin_was_not_allowed_to_pass_is_refused() {
    // The whole point: a plugin cannot weaken a CLI's permission posture unless
    // the user named that flag themselves.
    let args = vec!["--dangerously-skip-permissions".to_string()];
    let allowed = vec!["--resume".to_string()];
    let err = resume_command_line(Some("agent"), &args, &allowed).unwrap_err();
    assert!(
        err.to_string().contains("allowed_resume_flags"),
        "error should point at the allowlist: {err}"
    );
}

#[test]
fn an_argument_holding_shell_metacharacters_is_refused() {
    let allowed: Vec<String> = Vec::new();
    for hostile in [
        "a; rm -rf /",
        "$(id)",
        "`id`",
        "a b",
        "a'b",
        "a\nb",
        "a|b",
        "a>b",
        "a&b",
    ] {
        let args = vec![hostile.to_string()];
        assert!(
            resume_command_line(Some("agent"), &args, &allowed).is_err(),
            "should refuse {hostile:?}"
        );
    }
}

#[test]
fn too_many_or_too_long_arguments_are_refused() {
    let allowed: Vec<String> = Vec::new();
    let many: Vec<String> = (0..MAX_RESUME_ARGS + 1).map(|i| format!("v{i}")).collect();
    assert!(resume_command_line(Some("agent"), &many, &allowed).is_err());

    let long = vec!["v".repeat(MAX_RESUME_ARG_LEN + 1)];
    assert!(resume_command_line(Some("agent"), &long, &allowed).is_err());
}

#[test]
fn an_empty_argument_is_refused() {
    let allowed: Vec<String> = Vec::new();
    let args = vec![String::new()];
    assert!(resume_command_line(Some("agent"), &args, &allowed).is_err());
}

#[test]
fn a_pane_with_no_startup_command_cannot_be_relaunched() {
    let args = vec!["--resume".to_string()];
    let allowed = vec!["--resume".to_string()];
    // A bare shell has no session to resume, and nothing to reproduce.
    let err = resume_command_line(None, &args, &allowed).unwrap_err();
    assert!(err.to_string().contains("no startup command"), "{err}");
}

#[test]
fn quoting_neutralises_an_embedded_single_quote() {
    // Reached only if the charset check is ever loosened; the quoting has to be
    // correct on its own rather than relying on that check.
    assert_eq!(shell_quote("a'b"), "'a'\\''b'");
}
