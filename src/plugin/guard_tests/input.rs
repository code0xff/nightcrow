//! Rules 5-7: when input may be typed into a pane, and what it may contain.

use super::*;

#[test]
fn sending_input_to_a_pane_whose_process_exited_is_refused() {
    let mut g = guard();
    assert_eq!(
        g.judge(
            send(&token(), "hi\n"),
            Some(&exited_facts()),
            &[],
            Instant::now()
        )
        .expect_err("refused"),
        Refused::PaneNotRunning { pane: PANE }
    );
}

#[test]
fn sending_input_to_a_pane_that_is_still_talking_is_refused() {
    let mut g = guard();
    let facts = PaneFacts {
        idle: MIN_IDLE - Duration::from_millis(1),
        ..facts()
    };
    assert!(matches!(
        g.judge(send(&token(), "hi\n"), Some(&facts), &[], Instant::now()),
        Err(Refused::PaneBusy { pane: PANE, .. })
    ));
}

#[test]
fn input_exactly_at_the_idle_threshold_is_allowed() {
    let mut g = guard();
    // The threshold is inclusive: a pane quiet for exactly min_idle qualifies.
    assert_eq!(
        g.judge(send(&token(), "hi\n"), Some(&facts()), &[], Instant::now())
            .expect("allowed"),
        Approved::SendInput {
            pane: PANE,
            data: b"hi\n".to_vec()
        }
    );
}

#[test]
fn oversized_input_is_refused() {
    let mut g = guard();
    let data = "a".repeat(MAX_INPUT_BYTES + 1);
    assert_eq!(
        g.judge(send(&token(), &data), Some(&facts()), &[], Instant::now())
            .expect_err("refused"),
        Refused::InputTooLarge {
            pane: PANE,
            bytes: MAX_INPUT_BYTES + 1,
            limit: MAX_INPUT_BYTES
        }
    );
}

#[test]
fn input_exactly_at_the_size_limit_is_allowed() {
    let mut g = guard();
    let data = "a".repeat(MAX_INPUT_BYTES);
    assert!(
        g.judge(send(&token(), &data), Some(&facts()), &[], Instant::now())
            .is_ok()
    );
}

#[test]
fn empty_input_is_allowed() {
    let mut g = guard();
    assert!(
        g.judge(send(&token(), ""), Some(&facts()), &[], Instant::now())
            .is_ok()
    );
}

#[test]
fn only_carriage_return_newline_and_tab_are_accepted_as_control_characters() {
    let cases: &[(&str, bool)] = &[
        ("plain text", true),
        ("line\n", true),
        ("line\r\n", true),
        ("col\tcol", true),
        ("한글도 통과한다\n", true),
        ("\u{1b}[2J", false),
        ("bell\u{7}", false),
        ("nul\u{0}", false),
        ("del\u{7f}", false),
        ("csi\u{9b}31m", false),
        ("back\u{8}space", false),
        ("vertical\u{b}tab", false),
    ];
    for (data, allowed) in cases {
        let mut g = guard();
        let verdict = g.judge(send(&token(), data), Some(&facts()), &[], Instant::now());
        assert_eq!(
            verdict.is_ok(),
            *allowed,
            "input {data:?} should be allowed={allowed}"
        );
        if !allowed {
            assert!(matches!(
                verdict.expect_err("refused"),
                Refused::ControlCharacter { pane: PANE, .. }
            ));
        }
    }
}
