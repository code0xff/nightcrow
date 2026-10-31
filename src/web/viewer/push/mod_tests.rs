use super::*;

fn subscription(endpoint: &str) -> Subscription {
    super::store::tests::subscription(endpoint)
}

#[test]
fn only_a_usable_subscription_is_kept() {
    let push = PushService::start(None).unwrap();
    let mut bad = subscription("https://a.example/1");
    bad.endpoint = "http://a.example/1".into();
    assert!(push.subscribe(bad).is_err());
    assert!(!push.is_subscribed("http://a.example/1"));

    push.subscribe(subscription("https://a.example/1")).unwrap();
    assert!(push.is_subscribed("https://a.example/1"));
    assert!(push.unsubscribe("https://a.example/1"));
    assert!(!push.is_subscribed("https://a.example/1"));
}

#[test]
fn a_test_goes_only_to_a_subscribed_endpoint() {
    let push = PushService::start(None).unwrap();
    assert!(!push.send_test("https://a.example/1"));
}

#[test]
fn the_public_key_is_an_uncompressed_point() {
    use base64::Engine;
    let push = PushService::start(None).unwrap();
    let key = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(push.public_key())
        .unwrap();
    assert_eq!(key.len(), 65);
    assert_eq!(key[0], 4);
}

#[test]
fn a_notice_is_labelled_with_the_project_and_pane() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = crate::platform::paths::canonicalize_clean(dir.path())
        .unwrap()
        .to_string_lossy()
        .to_string();
    let prefs = tempfile::TempDir::new().unwrap();
    let session = crate::test_util::session_state(std::slice::from_ref(&path), prefs.path());
    let id = session.catalog().id_of_path(&path).expect("served");
    let name = session.catalog().get(&id).unwrap().name.clone();

    let notice = Notice {
        repo_path: path.clone(),
        pane: 4,
        pane_title: Some("claude".into()),
        message: Some("done".into()),
    };
    let payload = payload_for(&session, &notice);
    assert_eq!(payload.title, name);
    assert_eq!(payload.body, "claude: done");
    assert_eq!(payload.tag, format!("{id}:4"));
    assert_eq!(payload.repo.as_deref(), Some(id.as_str()));
    assert_eq!(payload.pane, Some(4));

    let bare = Notice {
        pane_title: None,
        message: None,
        ..notice
    };
    assert_eq!(
        payload_for(&session, &bare).body,
        "terminal 4 wants your attention"
    );
    session.catalog().shutdown();
}
