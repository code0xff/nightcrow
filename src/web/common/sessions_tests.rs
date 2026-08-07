use super::*;
use std::path::PathBuf;

fn tmp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nightcrow-test-{name}"));
    let _ = std::fs::remove_file(&dir);
    dir
}

#[test]
fn in_memory_store_issues_unique_valid_tokens() {
    let store = SessionStore::new();
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
    let store = SessionStore::new();
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
        let store = SessionStore::load(path.clone());
        let token = store.issue().unwrap();
        assert!(store.is_valid(&token));
    }
    // A new store loading the same file should recognise the token.
    let store = SessionStore::load(path.clone());
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
        let store = SessionStore::load(path.clone());
        keep = store.issue().unwrap();
        kill = store.issue().unwrap();
        store.revoke(&kill);
    }
    let store = SessionStore::load(path.clone());
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
    let store = SessionStore::load(path.clone());
    assert!(!store.is_valid("deadbeef"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn corrupt_file_starts_empty() {
    let path = tmp_path("persist-corrupt");
    std::fs::write(&path, "garbage no tab\n???\n").unwrap();
    let store = SessionStore::load(path.clone());
    let token = store.issue().unwrap();
    assert!(store.is_valid(&token));
    let _ = std::fs::remove_file(&path);
}
