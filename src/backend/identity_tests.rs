use super::*;

#[test]
fn minting_two_tokens_yields_distinct_values() {
    let a = PaneToken::new().expect("OS RNG");
    let b = PaneToken::new().expect("OS RNG");
    assert_ne!(a, b);
    assert_eq!(a.as_str().len(), TOKEN_BYTES * 2);
    assert!(a.as_str().chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn a_new_identity_starts_at_the_first_generation() {
    let id = PaneIdentity::new().expect("OS RNG");
    assert_eq!(id.generation, FIRST_GENERATION);
}

#[test]
fn advancing_an_identity_keeps_the_token_and_raises_the_generation() {
    let mut id = PaneIdentity::new().expect("OS RNG");
    let token = id.token.clone();
    id.advance();
    // The slot is the same slot; only the spawn inside it changed.
    assert_eq!(id.token, token);
    assert_eq!(id.generation, FIRST_GENERATION + 1);
}

#[test]
fn advancing_at_the_counter_ceiling_refuses_to_wrap() {
    let mut id = PaneIdentity::new().expect("OS RNG");
    id.generation = PaneGeneration::MAX;
    id.advance();
    // Wrapping would make a stale generation compare equal to the live one.
    assert_eq!(id.generation, PaneGeneration::MAX);
}

#[test]
fn a_token_serialises_as_a_plain_string() {
    // The plugin protocol carries tokens as JSON; a newtype that serialised as
    // an object or array would break every out-of-process reader.
    let token = PaneToken::new().expect("OS RNG");
    let json = serde_json::to_string(&token).expect("serialise");
    assert_eq!(json, format!("\"{}\"", token.as_str()));

    let back: PaneToken = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, token);
}
