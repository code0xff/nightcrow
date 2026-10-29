use super::*;

pub(crate) fn subscription(endpoint: &str) -> Subscription {
    let browser = crate::web::viewer::push::crypto::random_secret_key().unwrap();
    let key = crate::web::viewer::push::crypto::uncompressed(&browser.public_key());
    Subscription {
        endpoint: endpoint.into(),
        keys: SubscriptionKeys {
            p256dh: URL_SAFE_NO_PAD.encode(key),
            auth: URL_SAFE_NO_PAD.encode([9u8; 16]),
        },
    }
}

#[test]
fn a_browser_subscription_is_accepted() {
    for endpoint in [
        "https://fcm.googleapis.com/fcm/send/abc",
        "https://updates.push.services.mozilla.com/wpush/v2/x",
        "https://web.push.apple.com/Q1",
        "https://push.example.com:8443/x",
    ] {
        assert_eq!(subscription(endpoint).problem(), None, "{endpoint}");
    }
}

#[test]
fn an_endpoint_that_is_not_https_on_a_public_name_is_refused() {
    for endpoint in [
        "http://fcm.googleapis.com/x",
        "https://localhost/x",
        "https://printer.local/x",
        "https://127.0.0.1/x",
        "https://10.0.0.5:443/x",
        "https://[::1]/x",
        "https://intranet/x",
        "https://user@fcm.googleapis.com/x",
        "ftp://example.com/x",
    ] {
        assert!(subscription(endpoint).problem().is_some(), "{endpoint}");
    }
    let long = format!("https://a.example/{}", "x".repeat(MAX_ENDPOINT_LEN));
    assert!(subscription(&long).problem().is_some());
}

#[test]
fn malformed_keys_are_refused() {
    let mut s = subscription("https://a.example/x");
    s.keys.auth = URL_SAFE_NO_PAD.encode([1u8; 8]);
    assert!(s.problem().is_some());
    let mut s = subscription("https://a.example/x");
    s.keys.p256dh = "not base64!".into();
    assert!(s.problem().is_some());
}

#[test]
fn subscriptions_survive_a_reopen_and_refresh_by_endpoint() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("push").join("subscriptions.json");
    let store = Store::open(Some(path.clone()));
    store.add(subscription("https://a.example/1"));
    store.add(subscription("https://a.example/2"));
    let refreshed = subscription("https://a.example/1");
    store.add(refreshed.clone());

    let reopened = Store::open(Some(path));
    let all = reopened.all();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1], refreshed, "a refresh replaces, and moves to newest");
    assert!(reopened.remove("https://a.example/2"));
    assert!(!reopened.remove("https://a.example/2"));
    assert!(!reopened.contains("https://a.example/2"));
}

#[test]
fn the_oldest_subscription_makes_room_past_the_cap() {
    let store = Store::open(None);
    for i in 0..=MAX_SUBSCRIPTIONS {
        store.add(subscription(&format!("https://a.example/{i}")));
    }
    let all = store.all();
    assert_eq!(all.len(), MAX_SUBSCRIPTIONS);
    assert_eq!(all[0].endpoint, "https://a.example/1");
}

#[test]
fn an_unreadable_file_starts_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("subscriptions.json");
    std::fs::write(&path, "{").unwrap();
    assert!(Store::open(Some(path)).all().is_empty());
}
