//! The terminal-output fallback and the resume plan, sharing the fixtures in
//! `claude_tests`. Split from that file only to stay inside the 300-line limit.

use super::tests::{NOW, SESSION, ctx};
use super::*;

#[test]
fn a_usage_limit_line_in_output_reports_a_limit_exactly_once() {
    let mut claude = Claude::default();
    let text = "Claude usage limit reached. Your limit will reset later.\n";
    let event = claude
        .on_output(&ctx(1), text, NOW)
        .expect("a blocked account");
    assert_eq!(event.kind, LimitKind::UsageLimit);
    assert_eq!(
        event.resets_at, None,
        "no offset is printed, so no deadline"
    );
    assert_eq!(event.session_id, None);
    assert_eq!(
        claude.on_output(&ctx(1), text, NOW),
        None,
        "redraw must not refire"
    );
}

#[test]
fn output_matching_ignores_case() {
    let mut claude = Claude::default();
    let shouted = "YOU'VE HIT YOUR USAGE LIMIT";
    assert!(claude.on_output(&ctx(1), shouted, NOW).is_some());
}

#[test]
fn a_warning_that_the_limit_is_approaching_does_not_report_a_limit() {
    let mut claude = Claude::default();
    let text = "Heads up: you are approaching your usage limit for this window.\n";
    assert_eq!(claude.on_output(&ctx(1), text, NOW), None);
}

#[test]
fn a_needle_split_across_two_output_chunks_is_still_found() {
    let mut claude = Claude::default();
    assert_eq!(claude.on_output(&ctx(1), "Claude usage li", NOW), None);
    let event = claude.on_output(&ctx(1), "mit reached\n", NOW);
    assert!(event.is_some(), "the tail must span the chunk boundary");
}

#[test]
fn output_older_than_the_tail_budget_is_dropped() {
    let mut claude = Claude::default();
    assert_eq!(claude.on_output(&ctx(1), "usage li", NOW), None);
    let filler = "-".repeat(OUTPUT_TAIL_BYTES);
    assert_eq!(claude.on_output(&ctx(1), &filler, NOW), None);
    assert_eq!(claude.on_output(&ctx(1), "mit reached", NOW), None);
}

#[test]
fn an_exit_or_a_generation_change_rearms_the_output_latch() {
    let mut claude = Claude::default();
    let text = "Claude usage limit reached\n";
    assert!(claude.on_output(&ctx(1), text, NOW).is_some());
    assert_eq!(claude.on_output(&ctx(1), text, NOW), None);
    claude.on_exit(&ctx(1));
    assert!(
        claude.on_output(&ctx(1), text, NOW).is_some(),
        "exit re-arms"
    );
    assert!(
        claude.on_output(&ctx(2), text, NOW).is_some(),
        "respawn re-arms"
    );
}

#[test]
fn a_live_pane_is_resumed_by_typing_one_continuation_line() {
    let claude = Claude::default();
    let limit = LimitEvent::usage(Some(SESSION.to_string()), None, "d");
    let plan = claude.resume(&ctx(1), &limit, true);
    assert_eq!(plan, Some(ResumePlan::Input(NUDGE_INPUT.to_string())));
    assert!(NUDGE_INPUT.ends_with('\r'));
}

#[test]
fn an_exited_pane_relaunches_only_when_the_session_id_is_known() {
    let claude = Claude::default();
    let with_id = LimitEvent::usage(Some(SESSION.to_string()), None, "d");
    let expected = vec![RESUME_FLAG.to_string(), SESSION.to_string()];
    let plan = claude.resume(&ctx(1), &with_id, false);
    assert_eq!(plan, Some(ResumePlan::Relaunch(expected)));
    let without_id = LimitEvent::usage(None, None, "d");
    let plan = claude.resume(&ctx(1), &without_id, false);
    assert_eq!(plan, Some(ResumePlan::Hold(NO_SESSION_HOLD)));
}

#[test]
fn a_needs_human_limit_holds_even_while_the_pane_is_alive() {
    let claude = Claude::default();
    let limit = LimitEvent {
        session_id: Some(SESSION.to_string()),
        resets_at: None,
        kind: LimitKind::NeedsHuman,
        detail: "d".to_string(),
    };
    let hold = Some(ResumePlan::Hold(NEEDS_HUMAN_HOLD));
    assert_eq!(claude.resume(&ctx(1), &limit, true), hold);
    assert_eq!(claude.resume(&ctx(1), &limit, false), hold);
}
