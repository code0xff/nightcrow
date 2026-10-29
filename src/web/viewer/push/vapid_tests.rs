use super::*;
use p256::ecdsa::VerifyingKey;
use p256::ecdsa::signature::Verifier;

#[test]
fn a_key_is_made_once_and_read_back_after() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("push").join("vapid");
    let first = Vapid::load_or_create(&path).unwrap();
    let second = Vapid::load_or_create(&path).unwrap();
    assert_eq!(first.public_key(), second.public_key());
    assert_eq!(
        URL_SAFE_NO_PAD.decode(first.public_key()).unwrap().len(),
        65
    );
}

#[test]
fn an_unreadable_key_file_is_an_error_rather_than_replaced() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("vapid");
    std::fs::write(&path, "not a key").unwrap();
    assert!(Vapid::load_or_create(&path).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a key");
}

#[test]
fn the_authorization_is_a_verifiable_es256_token_for_the_endpoint_origin() {
    let secret = crate::web::viewer::push::crypto::random_secret_key().unwrap();
    let vapid = Vapid::from_secret(&secret);
    let now = UNIX_EPOCH + Duration::from_secs(1_000);
    let header = vapid
        .authorization("https://fcm.googleapis.com/fcm/send/abc", now)
        .unwrap();

    let (token, key) = header
        .strip_prefix("vapid t=")
        .and_then(|rest| rest.split_once(", k="))
        .unwrap();
    assert_eq!(key, vapid.public_key());
    let parts: Vec<&str> = token.split('.').collect();
    assert_eq!(parts.len(), 3);
    let claims: serde_json::Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(claims["aud"], "https://fcm.googleapis.com");
    assert_eq!(claims["exp"], 1_000 + 12 * 60 * 60);
    assert!(claims["sub"].as_str().unwrap().starts_with("https://"));

    let signature = Signature::from_slice(&URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).unwrap();
    let verifying = VerifyingKey::from(&SigningKey::from(&secret));
    verifying
        .verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature)
        .unwrap();
}

#[test]
fn an_origin_stops_at_the_path() {
    assert_eq!(origin("https://a.example/x/y"), Some("https://a.example"));
    assert_eq!(
        origin("https://a.example:8443"),
        Some("https://a.example:8443")
    );
    assert_eq!(origin("https:///x"), None);
    assert_eq!(origin("a.example/x"), None);
}
