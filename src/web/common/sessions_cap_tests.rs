use super::*;
use std::path::PathBuf;

fn tmp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nightcrow-test-{name}"));
    let _ = std::fs::remove_file(&dir);
    dir
}

fn a_day() -> Option<Duration> {
    Some(Duration::from_secs(24 * 3600))
}

#[test]
fn sweeping_keeps_what_is_still_live() {
    let now = SystemTime::now();
    let mut tokens = vec![
        Token {
            value: "past".to_string(),
            expiry: Some(now - Duration::from_secs(1)),
        },
        // Expiry is not "still live": `is_valid` demands strictly later.
        Token {
            value: "exactly-now".to_string(),
            expiry: Some(now),
        },
        Token {
            value: "future".to_string(),
            expiry: Some(now + Duration::from_secs(1)),
        },
    ];
    sweep(&mut tokens, now);
    assert_eq!(
        tokens.iter().map(|t| t.value.as_str()).collect::<Vec<_>>(),
        vec!["future"]
    );
}

#[test]
fn the_oldest_login_is_the_only_one_forgotten_past_the_cap() {
    let store = SessionStore::new(a_day());
    let oldest = store.issue().unwrap();
    let second = store.issue().unwrap();
    for _ in 1..MAX_SESSIONS {
        store.issue().unwrap();
    }

    assert_eq!(store.tokens.lock().unwrap().len(), MAX_SESSIONS);
    assert!(!store.is_valid(&oldest), "the oldest login still works");
    assert!(
        store.is_valid(&second),
        "the eviction took more than the oldest login"
    );
}

#[test]
fn an_evicted_login_is_gone_from_the_file_and_after_a_restart() {
    let path = tmp_path("persist-cap");
    let (evicted, newest) = {
        let store = SessionStore::load(path.clone(), None);
        let evicted = store.issue().unwrap();
        let mut newest = evicted.clone();
        for _ in 1..=MAX_SESSIONS {
            newest = store.issue().unwrap();
        }
        (evicted, newest)
    };

    let data = std::fs::read_to_string(&path).unwrap();
    assert_eq!(data.lines().count(), MAX_SESSIONS);
    assert!(
        !data.contains(evicted.as_str()),
        "the file still names an evicted login"
    );

    let store = SessionStore::load(path.clone(), None);
    assert_eq!(store.tokens.lock().unwrap().len(), MAX_SESSIONS);
    assert!(!store.is_valid(&evicted));
    assert!(store.is_valid(&newest));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_file_over_the_cap_is_trimmed_on_load() {
    let path = tmp_path("persist-cap-load");
    let mut data = String::new();
    for i in 0..MAX_SESSIONS + 2 {
        data.push_str(&format!("token-{i}\t{NEVER}\n"));
    }
    std::fs::write(&path, data).unwrap();

    let store = SessionStore::load(path.clone(), None);
    assert_eq!(store.tokens.lock().unwrap().len(), MAX_SESSIONS);
    assert!(!store.is_valid("token-0"));
    assert!(!store.is_valid("token-1"));
    assert!(store.is_valid(&format!("token-{}", MAX_SESSIONS + 1)));

    let data = std::fs::read_to_string(&path).unwrap();
    assert_eq!(data.lines().count(), MAX_SESSIONS);
    assert!(!data.contains("token-0"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_expired_login_never_pushes_a_live_one_past_the_cap() {
    let path = tmp_path("persist-cap-expired");
    let store = SessionStore::load(path.clone(), a_day());
    store.tokens.lock().unwrap().push(Token {
        value: "expired".to_string(),
        expiry: Some(SystemTime::now() - Duration::from_secs(1)),
    });
    let oldest_live = store.issue().unwrap();
    for _ in 1..MAX_SESSIONS {
        store.issue().unwrap();
    }

    assert!(store.is_valid(&oldest_live));
    let data = std::fs::read_to_string(&path).unwrap();
    assert!(!data.contains("expired"));
    assert_eq!(data.lines().count(), MAX_SESSIONS);
    let _ = std::fs::remove_file(&path);
}
