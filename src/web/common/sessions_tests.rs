use super::*;
use std::path::PathBuf;

fn tmp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nightcrow-test-{name}"));
    let _ = std::fs::remove_file(&dir);
    dir
}

/// The lifetime most tests do not care about: finite, and long enough that
/// nothing expires while the test runs.
fn a_day() -> Option<Duration> {
    Some(DEFAULT_SESSION_TTL)
}

#[test]
fn in_memory_store_issues_unique_valid_tokens() {
    let store = SessionStore::new(a_day());
    let a = store.issue().unwrap();
    let b = store.issue().unwrap();
    assert_ne!(a, b);
    assert_eq!(a.len(), 64);
    assert!(store.is_valid(&a));
    assert!(store.is_valid(&b));
    assert!(!store.is_valid("unknown"));
}

#[test]
fn revoke_stops_validating() {
    let store = SessionStore::new(a_day());
    let token = store.issue().unwrap();
    assert!(store.is_valid(&token));
    store.revoke(&token);
    assert!(!store.is_valid(&token));
    // Revoking an unknown token is a no-op.
    store.revoke("never-issued");
}

#[test]
fn persisted_tokens_survive_reload() {
    let path = tmp_path("persist-survive");
    {
        let store = SessionStore::load(path.clone(), a_day());
        let token = store.issue().unwrap();
        assert!(store.is_valid(&token));
    }
    // A new store loading the same file should recognise the token.
    let store = SessionStore::load(path.clone(), a_day());
    let data = std::fs::read_to_string(&path).unwrap();
    assert!(!data.is_empty());
    // Extract the token from the file to check it validates.
    let token = data.split('\t').next().unwrap().trim();
    assert!(store.is_valid(token));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn revoked_token_is_absent_after_reload() {
    let path = tmp_path("persist-revoke");
    let keep;
    let kill;
    {
        let store = SessionStore::load(path.clone(), a_day());
        keep = store.issue().unwrap();
        kill = store.issue().unwrap();
        store.revoke(&kill);
    }
    let store = SessionStore::load(path.clone(), a_day());
    assert!(store.is_valid(&keep));
    assert!(!store.is_valid(&kill));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn expired_tokens_are_not_loaded() {
    let path = tmp_path("persist-expired");
    // Write a token that expired in the past.
    let past = SystemTime::now() - Duration::from_secs(3600);
    let past_secs = past.duration_since(UNIX_EPOCH).unwrap().as_secs();
    let data = format!("deadbeef\t{past_secs}\n");
    std::fs::write(&path, data).unwrap();
    let store = SessionStore::load(path.clone(), a_day());
    assert!(!store.is_valid("deadbeef"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn writing_forgets_a_token_that_expired_while_the_daemon_ran() {
    let path = tmp_path("persist-sweep");
    let store = SessionStore::load(path.clone(), a_day());
    // A session nobody asks about again: the browser holding that cookie never
    // came back, so no `is_valid` call will ever reach this token.
    let stale = "stale-token";
    store.tokens.lock().unwrap().insert(
        stale.to_string(),
        Some(SystemTime::now() - Duration::from_secs(1)),
    );
    let live = store.issue().unwrap();

    let data = std::fs::read_to_string(&path).unwrap();
    assert!(
        !data.contains(stale),
        "the file still claims an expired session: {data}"
    );
    assert!(data.contains(&live));
    assert!(!store.is_valid(stale));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_store_without_a_file_forgets_expired_tokens_too() {
    // The fallback when the state directory cannot be opened. It keeps the same
    // map, and nothing restarts into a swept file to correct it.
    let store = SessionStore::new(a_day());
    store.tokens.lock().unwrap().insert(
        "stale-token".to_string(),
        Some(SystemTime::now() - Duration::from_secs(1)),
    );
    let live = store.issue().unwrap();
    assert_eq!(
        store.tokens.lock().unwrap().keys().collect::<Vec<_>>(),
        vec![&live]
    );
}

#[test]
fn sweeping_keeps_what_is_still_live() {
    let now = SystemTime::now();
    let mut tokens = HashMap::from([
        ("past".to_string(), Some(now - Duration::from_secs(1))),
        // Expiry is not "still live": `is_valid` demands strictly later.
        ("exactly-now".to_string(), Some(now)),
        ("future".to_string(), Some(now + Duration::from_secs(1))),
    ]);
    sweep(&mut tokens, now);
    assert_eq!(tokens.keys().collect::<Vec<_>>(), vec!["future"]);
}

#[test]
fn corrupt_file_starts_empty() {
    let path = tmp_path("persist-corrupt");
    std::fs::write(&path, "garbage no tab\n???\n").unwrap();
    let store = SessionStore::load(path.clone(), a_day());
    let token = store.issue().unwrap();
    assert!(store.is_valid(&token));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_nonsense_expiry_does_not_stop_the_server_from_starting() {
    let path = tmp_path("persist-overflow");
    std::fs::write(&path, format!("wild\t{}\n", u64::MAX)).unwrap();
    let store = SessionStore::load(path.clone(), a_day());
    assert!(!store.is_valid("wild"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_store_without_a_lifetime_issues_tokens_that_outlive_every_sweep() {
    let store = SessionStore::new(None);
    let token = store.issue().unwrap();
    assert_eq!(store.tokens.lock().unwrap().get(&token), Some(&None));

    // Far enough ahead that any finite lifetime would have run out.
    let mut tokens = store.tokens.lock().unwrap();
    sweep(
        &mut tokens,
        SystemTime::now() + Duration::from_secs(400 * 86400),
    );
    assert!(tokens.contains_key(&token));
    drop(tokens);

    assert!(store.is_valid(&token));
    // Logout is still what ends it.
    store.revoke(&token);
    assert!(!store.is_valid(&token));
}

#[test]
fn a_token_with_no_expiry_survives_a_restart() {
    let path = tmp_path("persist-never");
    let token = {
        let store = SessionStore::load(path.clone(), None);
        store.issue().unwrap()
    };
    let data = std::fs::read_to_string(&path).unwrap();
    assert_eq!(data.trim_end(), format!("{token}\t{NEVER}"));

    let store = SessionStore::load(path.clone(), None);
    assert!(store.is_valid(&token));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn tightening_the_lifetime_cuts_tokens_that_outlast_it_short() {
    let path = tmp_path("persist-clamp");
    let (immortal, long) = {
        let store = SessionStore::load(path.clone(), None);
        let immortal = store.issue().unwrap();
        let long = "long-lived".to_string();
        store.tokens.lock().unwrap().insert(
            long.clone(),
            Some(SystemTime::now() + Duration::from_secs(10 * 86400)),
        );
        store.persist();
        (immortal, long)
    };

    // Restarting under a one-hour lifetime.
    let hour = Duration::from_secs(3600);
    let store = SessionStore::load(path.clone(), Some(hour));
    let ceiling = SystemTime::now() + hour;
    let tokens = store.tokens.lock().unwrap();
    for token in [&immortal, &long] {
        let expiry = tokens.get(token).unwrap().expect("no longer immortal");
        assert!(
            expiry <= ceiling,
            "{token} still outlasts the tightened lifetime"
        );
    }
    drop(tokens);
    // Still usable right now — narrowing the policy is not a logout.
    assert!(store.is_valid(&immortal));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_restart_never_extends_a_token_that_is_nearly_out() {
    let path = tmp_path("persist-clamp-noextend");
    let nearly = SystemTime::now() + Duration::from_secs(60);
    let secs = nearly.duration_since(UNIX_EPOCH).unwrap().as_secs();
    std::fs::write(&path, format!("nearly\t{secs}\n")).unwrap();

    let store = SessionStore::load(path.clone(), Some(Duration::from_secs(86400)));
    let expiry = store.tokens.lock().unwrap()["nearly"].unwrap();
    assert!(
        expiry <= nearly,
        "a longer configured lifetime must not push an existing deadline out"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn the_cookie_lifetime_follows_the_configured_one_and_stays_within_the_browser_cap() {
    let hour = Duration::from_secs(3600);
    assert_eq!(SessionStore::new(Some(hour)).cookie_max_age_secs(), 3600);
    assert_eq!(SessionStore::new(a_day()).cookie_max_age_secs(), 86400);

    let cap = MAX_COOKIE_AGE.as_secs();
    // No expiry, and any lifetime past the cap, both ask for the longest a
    // browser will actually keep.
    assert_eq!(SessionStore::new(None).cookie_max_age_secs(), cap);
    assert_eq!(
        SessionStore::new(Some(MAX_COOKIE_AGE * 10)).cookie_max_age_secs(),
        cap
    );
}
