use super::file::*;
use super::*;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::time::UNIX_EPOCH;

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

#[test]
fn a_duplicated_line_keeps_the_earliest_expiry() {
    let path = tmp_path("persist-duplicate");
    let base = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // In the order a legacy hash-map file could produce: the later deadline
    // first.
    let (soon, later) = (base + 60, base + 3600);
    std::fs::write(&path, format!("dup\t{later}\ndup\t{soon}\nkept\t{NEVER}\n")).unwrap();

    let store = SessionStore::load(path.clone(), None);
    let tokens = store.tokens.lock().unwrap();
    let dup: Vec<_> = tokens.iter().filter(|t| t.value == "dup").collect();
    assert_eq!(dup.len(), 1, "the token is listed twice in memory");
    assert_eq!(
        dup[0].expiry,
        Some(UNIX_EPOCH + Duration::from_secs(base + 60)),
        "the duplicate kept its later deadline"
    );
    drop(tokens);
    let data = std::fs::read_to_string(&path).unwrap();
    let dup_lines = data.lines().filter(|line| line.starts_with("dup\t"));
    assert_eq!(dup_lines.count(), 1);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_revoked_login_makes_room_without_evicting_a_live_one() {
    let path = tmp_path("persist-cap-revoke");
    let store = SessionStore::load(path.clone(), None);
    let mut issued = Vec::new();
    for _ in 0..MAX_SESSIONS {
        issued.push(store.issue().unwrap());
    }
    store.revoke(&issued[0]);
    let fresh = store.issue().unwrap();

    assert_eq!(store.tokens.lock().unwrap().len(), MAX_SESSIONS);
    assert!(!store.is_valid(&issued[0]));
    for token in &issued[1..] {
        assert!(store.is_valid(token), "a live login was evicted");
    }
    assert!(store.is_valid(&fresh));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn an_over_cap_file_is_clamped_and_trimmed_together() {
    let path = tmp_path("persist-cap-clamp");
    let base = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    // Out of eviction order: the soonest deadline sits between the
    // never-expiring lines that a legacy file would have written first.
    let mut data = format!(
        "immortal-0\t{NEVER}\nsoon\t{}\nimmortal-1\t{NEVER}\n",
        base + 60
    );
    for i in 2..=MAX_SESSIONS {
        data.push_str(&format!("immortal-{i}\t{NEVER}\n"));
    }
    std::fs::write(&path, data).unwrap();

    let ttl = Duration::from_secs(3600);
    let store = SessionStore::load(path.clone(), Some(ttl));
    let ceiling = SystemTime::now() + ttl;
    let tokens = store.tokens.lock().unwrap();
    assert_eq!(tokens.len(), MAX_SESSIONS);
    // "soon" was already inside the lifetime and is still the first to go;
    // the never-expiring lines are clamped to the configured ceiling.
    assert!(
        tokens
            .iter()
            .all(|t| t.value != "soon" && t.value != "immortal-0")
    );
    assert!(
        tokens
            .iter()
            .all(|t| t.expiry.is_some_and(|at| at <= ceiling))
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.value == format!("immortal-{MAX_SESSIONS}"))
    );
    drop(tokens);

    let data = std::fs::read_to_string(&path).unwrap();
    assert_eq!(data.lines().count(), MAX_SESSIONS);
    assert!(
        !data.contains(&format!("\t{NEVER}\n")),
        "a clamped token still reads as never"
    );
    assert!(!data.contains("soon"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn revoking_an_unknown_token_leaves_the_file_alone() {
    let path = tmp_path("persist-revoke-unknown");
    std::fs::write(&path, format!("known\t{NEVER}\n")).unwrap();
    let store = SessionStore::load(path.clone(), None);

    // A rewrite serializes the in-memory tokens over the file, so this marker
    // survives only if `revoke` wrote nothing.
    let sentinel = "sentinel-not-a-token\n";
    std::fs::write(&path, sentinel).unwrap();
    store.revoke("never-issued");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), sentinel);

    // A token the store does hold still reaches the file.
    store.revoke("known");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn concurrent_writers_leave_the_file_matching_memory() {
    let path = tmp_path("persist-concurrent");
    std::fs::write(&path, "").unwrap();
    let store = Arc::new(SessionStore::load(path.clone(), None));
    let barrier = Arc::new(Barrier::new(4));
    let handoff = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut threads = Vec::new();
    for _ in 0..4 {
        let store = Arc::clone(&store);
        let barrier = Arc::clone(&barrier);
        let handoff = Arc::clone(&handoff);
        threads.push(std::thread::spawn(move || {
            barrier.wait();
            for round in 0..12 {
                let token = store.issue().unwrap();
                handoff.lock().unwrap().push(token);
                if round % 2 == 0 {
                    // Any thread's token may be the victim, so issue and
                    // revoke race across writers.
                    let victim = handoff.lock().unwrap().pop();
                    if let Some(victim) = victim {
                        store.revoke(&victim);
                    }
                }
            }
        }));
    }
    for thread in threads {
        thread.join().unwrap();
    }

    let mut in_memory: Vec<String> = store
        .tokens
        .lock()
        .unwrap()
        .iter()
        .map(|t| t.value.clone())
        .collect();
    in_memory.sort();
    let mut on_disk: Vec<String> = SessionStore::load(path.clone(), None)
        .tokens
        .lock()
        .unwrap()
        .iter()
        .map(|t| t.value.clone())
        .collect();
    on_disk.sort();
    assert_eq!(
        on_disk, in_memory,
        "the last write lost a login or restored a revoked one"
    );
    let _ = std::fs::remove_file(&path);
}
