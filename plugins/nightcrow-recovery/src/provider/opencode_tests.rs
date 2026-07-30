use super::*;
use crate::provider::LimitKind;
use serde_json::Value;

/// Fixed "now", well inside the plausible epoch band.
const NOW: i64 = 1_800_000_000;

/// Session id used wherever the id itself is not what a test is about.
const SESSION: &str = "ses_1";

/// Status bodies handed out one per fetch; the last one repeats forever, so a
/// test can keep polling without restating the snapshot.
#[derive(Debug)]
struct Scripted {
    bodies: Vec<String>,
    at: usize,
}

impl Scripted {
    fn new(bodies: Vec<String>) -> Box<Self> {
        Box::new(Self { bodies, at: 0 })
    }
}

impl StatusSource for Scripted {
    fn fetch(&mut self) -> anyhow::Result<String> {
        let body = self
            .bodies
            .get(self.at)
            .or_else(|| self.bodies.last())
            .cloned()
            .unwrap_or_default();
        self.at += 1;
        Ok(body)
    }
}

/// Stands in for "the user is not running the server".
#[derive(Debug)]
struct Unreachable;

impl StatusSource for Unreachable {
    fn fetch(&mut self) -> anyhow::Result<String> {
        anyhow::bail!("connection refused")
    }
}

fn ctx(generation: PaneGeneration) -> PaneContext {
    PaneContext {
        token: "t0".to_string(),
        generation,
        cwd: "/repo".to_string(),
        command: Some("opencode".to_string()),
    }
}

fn wrap(id: &str, status: Value) -> String {
    let mut map = serde_json::Map::new();
    map.insert(id.to_string(), status);
    Value::Object(map).to_string()
}

fn retry_body(id: &str, next: Option<i64>) -> String {
    let mut status = serde_json::json!({"type": "retry", "attempt": 2});
    if let Some(next) = next {
        status["next"] = next.into();
    }
    wrap(id, status)
}

fn idle_body(id: &str) -> String {
    wrap(id, serde_json::json!({"type": "idle"}))
}

fn adapter(bodies: Vec<String>) -> OpenCode {
    OpenCode::with_status_source(Scripted::new(bodies))
}

#[test]
fn the_adapter_reports_itself_as_opencode() {
    assert_eq!(adapter(vec![]).name(), "opencode");
}

#[test]
fn a_retry_status_never_produces_an_event_no_matter_how_many_polls() {
    let mut oc = adapter(vec![retry_body(SESSION, Some(30_000))]);
    for tick in 0..6 {
        let at = NOW + tick * MIN_POLL_INTERVAL_SECS;
        assert_eq!(oc.poll(&ctx(1), at), None, "poll at +{tick} intervals");
    }
}

#[test]
fn a_retry_going_idle_produces_exactly_one_usage_limit_event() {
    let mut oc = adapter(vec![retry_body(SESSION, None), idle_body(SESSION)]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    let event = oc.poll(&ctx(1), NOW + 5).expect("idle after retry reports");
    assert_eq!(event.kind, LimitKind::UsageLimit);
    assert_eq!(event.session_id.as_deref(), Some(SESSION));
    assert_eq!(oc.poll(&ctx(1), NOW + 10), None, "second event suppressed");
}

#[test]
fn a_retry_then_a_process_exit_produces_exactly_one_event() {
    let mut oc = adapter(vec![retry_body(SESSION, None)]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    oc.on_exit(&ctx(1));
    let event = oc.poll(&ctx(1), NOW + 5).expect("exit after retry reports");
    assert_eq!(event.session_id.as_deref(), Some(SESSION));
    assert_eq!(oc.poll(&ctx(1), NOW + 10), None);
}

#[test]
fn an_exit_without_a_retry_ever_seen_produces_no_event() {
    let mut oc = adapter(vec![wrap(SESSION, serde_json::json!({"type": "busy"}))]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    oc.on_exit(&ctx(1));
    assert_eq!(oc.poll(&ctx(1), NOW + 5), None);
    assert_eq!(oc.poll(&ctx(1), NOW + 10), None);
}

#[test]
fn an_event_carries_a_deadline_only_when_the_resolved_next_is_in_the_future() {
    let future = (NOW + 3600) * 1_000;
    let mut ahead = adapter(vec![retry_body(SESSION, Some(future)), idle_body(SESSION)]);
    assert_eq!(ahead.poll(&ctx(1), NOW), None);
    let event = ahead.poll(&ctx(1), NOW + 5).expect("reports");
    assert_eq!(event.resets_at, Some(NOW + 3600));

    // 500 ms resolves to `NOW`, which is already past by the time it is emitted.
    let mut elapsed = adapter(vec![retry_body(SESSION, Some(500)), idle_body(SESSION)]);
    assert_eq!(elapsed.poll(&ctx(1), NOW), None);
    let event = elapsed.poll(&ctx(1), NOW + 5).expect("reports");
    assert_eq!(event.resets_at, None);
}

#[test]
fn an_event_carries_no_deadline_when_the_status_gave_no_next() {
    let mut oc = adapter(vec![retry_body(SESSION, None), idle_body(SESSION)]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    assert_eq!(oc.poll(&ctx(1), NOW + 5).expect("reports").resets_at, None);
}

#[test]
fn a_session_id_that_is_not_command_line_safe_is_dropped_and_resume_holds() {
    let long = "a".repeat(MAX_SESSION_ID_BYTES + 1);
    for id in ["ses 1", "ses;rm", "ses$(id)", "../ses", &long] {
        let mut oc = adapter(vec![retry_body(id, None), idle_body(id)]);
        assert_eq!(oc.poll(&ctx(1), NOW), None);
        let event = oc.poll(&ctx(1), NOW + 5).expect("reports");
        assert_eq!(event.session_id, None, "id {id:?} must not survive");
        assert_eq!(
            oc.resume(&ctx(1), &event, false),
            Some(ResumePlan::Hold(NO_SESSION_HOLD))
        );
    }
}

#[test]
fn a_generation_change_re_arms_the_once_per_generation_latch() {
    let mut oc = adapter(vec![
        retry_body(SESSION, None),
        idle_body(SESSION),
        retry_body(SESSION, None),
        idle_body(SESSION),
    ]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    assert!(oc.poll(&ctx(1), NOW + 5).is_some());
    assert_eq!(oc.poll(&ctx(2), NOW + 10), None);
    assert!(oc.poll(&ctx(2), NOW + 15).is_some(), "latch re-armed");
}

#[test]
fn a_generation_change_forgets_a_retry_seen_before_it() {
    let mut oc = adapter(vec![retry_body(SESSION, None), idle_body(SESSION)]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    // The retry belonged to the old process, so the new generation's idle
    // snapshot is not the end of anything this adapter watched.
    assert_eq!(oc.poll(&ctx(2), NOW + 5), None);
    assert_eq!(oc.poll(&ctx(2), NOW + 10), None);
}

#[test]
fn an_exit_recorded_against_an_older_generation_does_not_report() {
    let mut oc = adapter(vec![retry_body(SESSION, None)]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    oc.on_exit(&ctx(2));
    assert_eq!(oc.poll(&ctx(2), NOW + 5), None);
}

#[test]
fn polls_closer_together_than_the_minimum_interval_do_not_reach_the_source() {
    let mut oc = adapter(vec![retry_body(SESSION, None), idle_body(SESSION)]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    for early in 1..MIN_POLL_INTERVAL_SECS {
        assert_eq!(oc.poll(&ctx(1), NOW + early), None, "+{early}s is too soon");
    }
    assert!(oc.poll(&ctx(1), NOW + MIN_POLL_INTERVAL_SECS).is_some());
}

#[test]
fn an_unreachable_server_produces_no_event() {
    let mut oc = OpenCode::with_status_source(Box::new(Unreachable));
    for tick in 0..4 {
        let at = NOW + tick * MIN_POLL_INTERVAL_SECS;
        assert_eq!(oc.poll(&ctx(1), at), None);
    }
    oc.on_exit(&ctx(1));
    assert_eq!(oc.poll(&ctx(1), NOW + 100), None);
}

#[test]
fn an_unreadable_status_body_produces_no_event() {
    let mut oc = adapter(vec!["not json".to_string(), "[]".to_string()]);
    assert_eq!(oc.poll(&ctx(1), NOW), None);
    assert_eq!(oc.poll(&ctx(1), NOW + 5), None);
}

#[test]
fn terminal_output_never_produces_an_event() {
    let mut oc = adapter(vec![]);
    for text in ["retrying in 2s", "usage limit reached", "rate_limit_error"] {
        assert_eq!(oc.on_output(&ctx(1), text, NOW), None, "{text}");
    }
}

#[test]
fn resume_holds_while_the_pane_is_still_alive() {
    let event = LimitEvent::usage(Some(SESSION.to_string()), None, "d");
    let plan = adapter(vec![]).resume(&ctx(1), &event, true);
    assert_eq!(plan, Some(ResumePlan::Hold(ALIVE_HOLD)));
}

#[test]
fn resume_relaunches_with_the_session_flag_once_the_pane_has_exited() {
    let event = LimitEvent::usage(Some(SESSION.to_string()), None, "d");
    let plan = adapter(vec![]).resume(&ctx(1), &event, false);
    let want = vec!["--session".to_string(), SESSION.to_string()];
    assert_eq!(plan, Some(ResumePlan::Relaunch(want)));
}

#[test]
fn resume_holds_when_no_session_id_is_known() {
    let event = LimitEvent::usage(None, None, "d");
    let plan = adapter(vec![]).resume(&ctx(1), &event, false);
    assert_eq!(plan, Some(ResumePlan::Hold(NO_SESSION_HOLD)));
}

#[test]
fn observe_command_takes_the_port_from_a_port_flag() {
    for (command, want) in [
        ("opencode --port 5000", 5000),
        ("opencode -p 6000", 6000),
        ("opencode --port=7000", 7000),
        ("opencode run --model x --port 5173", 5173),
    ] {
        let mut oc = adapter(vec![]);
        oc.observe_command(command);
        assert_eq!(oc.port(), want, "{command}");
    }
}

#[test]
fn observe_command_ignores_a_port_it_cannot_use() {
    let mut oc = adapter(vec![]);
    oc.observe_command("opencode --port 5000");
    for command in [
        "opencode --port abc",
        "opencode --port 99999",
        "opencode --port 0",
        "opencode --port",
        "opencode --port=",
    ] {
        oc.observe_command(command);
        assert_eq!(oc.port(), 5000, "{command} must not change the port");
    }
}

#[test]
fn observe_command_leaves_the_port_alone_when_no_flag_is_present() {
    let mut oc = adapter(vec![]);
    let before = oc.port();
    oc.observe_command("opencode run --model anthropic/claude --print");
    assert_eq!(oc.port(), before);
    assert_eq!(port_from_command("opencode"), None);
}
