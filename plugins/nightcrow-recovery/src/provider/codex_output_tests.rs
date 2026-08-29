//! Terminal-output fallback and resume tests. Neither touches the filesystem, so
//! they need no `CODEX_HOME` tree; the rollout-tailing cases live in
//! `codex_tests.rs`.

use super::pane::{OUTPUT_DETAIL, USAGE_LIMIT_NEEDLES};
use super::rollout::USAGE_LIMIT_ERROR_INFO;
use super::*;
use crate::protocol::PaneGeneration;

const UUID: &str = "0199cbb1-2b70-7f11-9f0f-0f8e9d1c2b3a";
/// Any plausible "now"; the output path never reads a time out of text, so the
/// value cannot change an outcome here.
const NOW: i64 = 1_800_000_000;
/// A home that does not exist: these tests must not depend on one.
const NO_HOME: &str = "/nonexistent/codex-home";

fn ctx(generation: PaneGeneration) -> PaneContext {
    PaneContext {
        token: "pane-0".to_string(),
        generation,
        cwd: "/repo".to_string(),
        command: Some("codex".to_string()),
    }
}

fn adapter() -> Codex {
    Codex::with_home(PathBuf::from(NO_HOME))
}

#[test]
fn the_adapter_is_named_codex() {
    assert_eq!(adapter().name(), "codex");
}

#[test]
fn every_terminal_output_needle_fires_once() {
    for needle in USAGE_LIMIT_NEEDLES {
        let mut codex = adapter();
        // Upper-cased, because matching must not depend on codex's casing.
        let shouted = needle.to_uppercase();
        let event = codex
            .on_output(&ctx(1), &shouted, NOW)
            .expect("the needle fires");
        assert_eq!(event.detail, OUTPUT_DETAIL);
        assert_eq!(event.resets_at, None, "no time is ever read out of text");
        assert_eq!(event.session_id, None);
        assert_eq!(codex.on_output(&ctx(1), &shouted, NOW), None);
    }
}

#[test]
fn a_repeated_output_chunk_does_not_fire_again() {
    let mut codex = adapter();
    let chunk = "Codex: You've hit your usage limit. Try again at 3:45 PM.";
    assert!(codex.on_output(&ctx(1), chunk, NOW).is_some());
    assert_eq!(codex.on_output(&ctx(1), chunk, NOW), None);
}

#[test]
fn a_needle_split_across_two_chunks_is_found() {
    let mut codex = adapter();
    let (head, tail) = "You've hit your usage limit".split_at(12);
    assert_eq!(codex.on_output(&ctx(1), head, NOW), None);
    assert!(codex.on_output(&ctx(1), tail, NOW).is_some());
}

#[test]
fn ordinary_output_never_fires() {
    let mut codex = adapter();
    let chunk = "reading your usage of the limit parser\n";
    assert_eq!(codex.on_output(&ctx(1), chunk, NOW), None);
}

#[test]
fn a_new_generation_re_arms_the_output_latch() {
    let mut codex = adapter();
    let chunk = "You've hit your usage limit";
    assert!(codex.on_output(&ctx(1), chunk, NOW).is_some());
    assert!(codex.on_output(&ctx(2), chunk, NOW).is_some());
}

#[test]
fn on_exit_re_arms_the_output_latch() {
    let mut codex = adapter();
    let chunk = "Quota exceeded. Check your plan and billing details.";
    assert!(codex.on_output(&ctx(1), chunk, NOW).is_some());
    codex.on_exit(&ctx(1));
    assert!(codex.on_output(&ctx(1), chunk, NOW).is_some());
}

#[test]
fn on_exit_for_an_unknown_pane_or_an_old_generation_does_nothing() {
    let mut codex = adapter();
    codex.on_exit(&ctx(1));
    assert!(
        codex
            .on_output(&ctx(1), "You've hit your usage limit", NOW)
            .is_some()
    );
    // Exit of a generation this pane has moved on from must not re-arm the latch.
    codex.on_exit(&ctx(0));
    assert_eq!(
        codex.on_output(&ctx(1), "You've hit your usage limit", NOW),
        None
    );
}

#[test]
fn resume_while_the_pane_is_alive_holds() {
    let limit = LimitEvent::usage(Some(UUID.to_string()), None, USAGE_LIMIT_ERROR_INFO);
    assert_eq!(
        adapter().resume(&ctx(1), &limit, true),
        Some(ResumePlan::Hold(HOLD_ALIVE))
    );
}

#[test]
fn resume_after_exit_with_a_session_id_relaunches_with_resume_and_the_id() {
    let limit = LimitEvent::usage(Some(UUID.to_string()), None, USAGE_LIMIT_ERROR_INFO);
    assert_eq!(
        adapter().resume(&ctx(1), &limit, false),
        Some(ResumePlan::Relaunch(vec![
            "resume".to_string(),
            UUID.to_string()
        ]))
    );
}

#[test]
fn resume_after_exit_without_a_session_id_holds() {
    let limit = LimitEvent::usage(None, None, USAGE_LIMIT_ERROR_INFO);
    assert_eq!(
        adapter().resume(&ctx(1), &limit, false),
        Some(ResumePlan::Hold(HOLD_NO_ID))
    );
}

#[test]
fn resume_refuses_a_session_id_that_is_not_argv_safe() {
    let limit = LimitEvent::usage(Some("a b; rm -rf /".to_string()), None, "x");
    assert_eq!(
        adapter().resume(&ctx(1), &limit, false),
        Some(ResumePlan::Hold(HOLD_NO_ID))
    );
}

#[test]
fn a_session_id_the_output_path_learned_survives_into_the_resume_plan() {
    let mut codex = adapter();
    assert!(
        codex
            .on_output(&ctx(1), "You've hit your usage limit", NOW)
            .is_some()
    );
    // Nothing bound a rollout, so no id was ever learned: hold rather than guess.
    let limit = LimitEvent::usage(None, None, OUTPUT_DETAIL);
    assert_eq!(
        codex.resume(&ctx(1), &limit, false),
        Some(ResumePlan::Hold(HOLD_NO_ID))
    );
}
