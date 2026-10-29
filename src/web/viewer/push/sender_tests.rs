use super::*;

#[test]
fn a_tag_is_sent_once_per_window_and_others_are_not_held_back() {
    let mut last = HashMap::new();
    let start = Instant::now();
    assert!(due(&mut last, "r:1", start));
    assert!(!due(&mut last, "r:1", start + Duration::from_secs(3)));
    assert!(due(&mut last, "r:2", start + Duration::from_secs(3)));
    assert!(due(&mut last, "r:1", start + COALESCE_WINDOW));
}

#[test]
fn push_service_answers_are_classified() {
    assert_eq!(classify(201), Outcome::Delivered);
    assert_eq!(classify(410), Outcome::Gone);
    assert_eq!(classify(404), Outcome::Gone);
    assert!(matches!(classify(413), Outcome::Failed(_)));
    assert!(matches!(classify(429), Outcome::Failed(_)));
}

#[test]
fn a_topic_is_short_url_safe_and_stable_per_tag() {
    let one = topic("repo-1:3");
    assert_eq!(one.len(), 32);
    assert!(
        one.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    );
    assert_eq!(one, topic("repo-1:3"));
    assert_ne!(one, topic("repo-1:4"));
}
