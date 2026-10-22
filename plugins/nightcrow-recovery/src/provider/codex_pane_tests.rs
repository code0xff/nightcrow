use super::*;
use serde_json::{Value, json};
use std::time::Instant;

const NOW: i64 = 1_800_000_000;
const SESSION_A: &str = "session-a";
const SESSION_B: &str = "session-b";

fn line(kind: &str, payload: Value) -> Vec<u8> {
    format!("{}\n", json!({"type": kind, "payload": payload})).into_bytes()
}

fn meta(id: &str) -> Vec<u8> {
    line("session_meta", json!({"id": id}))
}

fn token_count(reset: i64) -> Vec<u8> {
    line(
        "token_count",
        json!({"rate_limits": {"primary": {"resets_at": reset}}}),
    )
}

fn limit() -> Vec<u8> {
    line(
        "turn_complete",
        json!({"error": {"codex_error_info": USAGE_LIMIT_ERROR_INFO}}),
    )
}

fn append(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(bytes);
}

#[test]
fn scans_many_short_records_and_carries_a_partial_trailing_record() {
    let mut state = PaneState::new(1, NOW);
    let partial = b"{\"type\":\"session_meta\",\"payload\":{\"id\":\"split-session";
    let record_count = (MAX_POLL_READ_BYTES - partial.len()) / 3;
    let mut first_poll = b"{}\n".repeat(record_count);
    append(&mut first_poll, partial);
    assert!(first_poll.len() <= MAX_POLL_READ_BYTES);

    assert_eq!(state.consume(&first_poll, NOW), None);
    assert_eq!(state.session_id(), None);
    assert_eq!(state.pending, partial);

    let mut second_poll = b"-id\"}}\n".to_vec();
    append(&mut second_poll, &limit());
    let event = state
        .consume(&second_poll, NOW)
        .expect("the completed limit record is applied");
    assert_eq!(event.session_id.as_deref(), Some("split-session-id"));
    assert!(state.pending.is_empty());
}

#[test]
fn applies_records_in_order_and_returns_the_first_limit_event() {
    let first_reset = NOW + 3_600;
    let second_reset = NOW + 7_200;
    let mut state = PaneState::new(1, NOW);
    let mut input = Vec::new();
    append(&mut input, b"{bad json\n\xff\n");
    for record in [
        meta(SESSION_A),
        token_count(first_reset),
        limit(),
        meta(SESSION_B),
        token_count(second_reset),
        limit(),
    ] {
        append(&mut input, &record);
    }

    let event = state
        .consume(&input, NOW)
        .expect("the first limit event is retained");
    assert_eq!(event.session_id.as_deref(), Some(SESSION_A));
    assert_eq!(event.resets_at, Some(first_reset));
    assert_eq!(state.session_id.as_deref(), Some(SESSION_B));
    assert_eq!(state.resets_at, Some(second_reset));
    assert!(state.pending.is_empty());
}

#[test]
fn drops_an_oversized_incomplete_record_and_accepts_later_records() {
    let mut state = PaneState::new(1, NOW);
    assert_eq!(state.consume(&vec![b'x'; MAX_RECORD_BYTES + 1], NOW), None);
    assert!(state.pending.is_empty());

    let mut input = meta(SESSION_A);
    append(&mut input, &limit());
    let event = state
        .consume(&input, NOW)
        .expect("a later complete record remains readable");
    assert_eq!(event.session_id.as_deref(), Some(SESSION_A));
}

#[test]
#[ignore = "manual linear-scan versus repeated-front-drain timing comparison"]
fn compare_many_short_record_consumers() {
    let input = b"{}\n".repeat(MAX_POLL_READ_BYTES / 3);
    let mut state = PaneState::new(1, NOW);
    let started = Instant::now();
    assert_eq!(state.consume(&input, NOW), None);
    let linear = started.elapsed();

    let mut pending = input.clone();
    let mut consumed = 0;
    let started = Instant::now();
    while let Some(relative_nl) = pending[consumed..].iter().position(|b| *b == b'\n') {
        let nl = consumed + relative_nl;
        let _ = std::str::from_utf8(&pending[consumed..nl])
            .ok()
            .and_then(|record| classify_line(record, NOW));
        consumed = nl + 1;
        pending.drain(..consumed);
        consumed = 0;
    }
    let repeated_drain = started.elapsed();
    eprintln!("linear={linear:?}; repeated front drain={repeated_drain:?}");
}
