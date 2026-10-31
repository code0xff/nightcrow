//! The viewer's Web Push routes. The test server's push service keeps its key
//! and subscriptions in memory; nothing here reaches a real push service.

use super::{body_of, get, login, post, server};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

fn subscription_json(endpoint: &str) -> String {
    let browser = p256::SecretKey::from_slice(&[7u8; 32]).unwrap();
    let key = browser.public_key().to_sec1_bytes();
    serde_json::json!({
        "endpoint": endpoint,
        "keys": { "p256dh": URL_SAFE_NO_PAD.encode(key), "auth": URL_SAFE_NO_PAD.encode([1u8; 16]) },
    })
    .to_string()
}

fn endpoint_json(endpoint: &str) -> String {
    serde_json::json!({ "endpoint": endpoint }).to_string()
}

#[test]
fn every_push_route_requires_authentication() {
    let server = server(&[]);
    assert!(get(server.addr(), "/api/push/key", None).starts_with("HTTP/1.1 401"));
    for path in [
        "/api/push/subscribe",
        "/api/push/unsubscribe",
        "/api/push/status",
        "/api/push/test",
    ] {
        let response = post(
            server.addr(),
            path,
            &endpoint_json("https://a.example/1"),
            None,
        );
        assert!(response.starts_with("HTTP/1.1 401"), "{path}: {response}");
    }
}

#[test]
fn the_key_is_the_uncompressed_point_a_page_subscribes_with() {
    let server = server(&[]);
    let token = login(server.addr());
    let response = get(server.addr(), "/api/push/key", Some(&token));
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let body: serde_json::Value = serde_json::from_str(body_of(&response)).unwrap();
    assert_eq!(body["version"], crate::web::viewer::dto::PROTOCOL_VERSION);
    let key = URL_SAFE_NO_PAD
        .decode(body["public_key"].as_str().unwrap())
        .unwrap();
    assert_eq!(key.len(), 65);
}

#[test]
fn a_subscription_is_kept_reported_tested_and_dropped() {
    let server = server(&[]);
    let token = login(server.addr());
    let endpoint = "https://push.example.com/abc";
    let status = |token: &str| {
        let response = post(
            server.addr(),
            "/api/push/status",
            &endpoint_json(endpoint),
            Some(token),
        );
        serde_json::from_str::<serde_json::Value>(body_of(&response)).unwrap()["subscribed"].clone()
    };

    assert_eq!(status(&token), false);
    let test = post(
        server.addr(),
        "/api/push/test",
        &endpoint_json(endpoint),
        Some(&token),
    );
    assert!(test.starts_with("HTTP/1.1 404"), "{test}");

    let added = post(
        server.addr(),
        "/api/push/subscribe",
        &subscription_json(endpoint),
        Some(&token),
    );
    assert!(added.starts_with("HTTP/1.1 200"), "{added}");
    assert_eq!(status(&token), true);
    let test = post(
        server.addr(),
        "/api/push/test",
        &endpoint_json(endpoint),
        Some(&token),
    );
    assert!(test.starts_with("HTTP/1.1 202"), "{test}");

    let removed = post(
        server.addr(),
        "/api/push/unsubscribe",
        &endpoint_json(endpoint),
        Some(&token),
    );
    assert!(removed.starts_with("HTTP/1.1 200"), "{removed}");
    assert_eq!(status(&token), false);
}

#[test]
fn an_unusable_subscription_is_refused_with_the_reason() {
    let server = server(&[]);
    let token = login(server.addr());
    let response = post(
        server.addr(),
        "/api/push/subscribe",
        &subscription_json("http://127.0.0.1/x"),
        Some(&token),
    );
    assert!(response.starts_with("HTTP/1.1 400"), "{response}");
    assert!(body_of(&response).contains("https"), "{response}");
    let malformed = post(server.addr(), "/api/push/subscribe", "{}", Some(&token));
    assert!(malformed.starts_with("HTTP/1.1 400"), "{malformed}");
}

#[test]
fn a_get_cannot_change_a_subscription() {
    let server = server(&[]);
    let token = login(server.addr());
    let response = get(server.addr(), "/api/push/subscribe", Some(&token));
    assert!(response.starts_with("HTTP/1.1 404"), "{response}");
}
