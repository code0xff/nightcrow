use crate::config::{Config, parse_leader};
use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn input_leader_defaults_to_ctrl_f() {
    let cfg = Config::default();
    assert_eq!(cfg.input.leader, "ctrl+f");
    let leader = parse_leader(&cfg.input.leader).unwrap();
    assert_eq!(leader.code, KeyCode::Char('f'));
    assert!(leader.modifiers.contains(KeyModifiers::CONTROL));
}

#[test]
fn parse_leader_rejects_unencodable_ctrl_chords() {
    // Digits and punctuation have no single control-byte encoding, so they
    // would break `<L><L>` literal pass-through and must be rejected.
    for spec in ["ctrl+1", "ctrl+-", "ctrl+/", "ctrl+@"] {
        assert!(
            parse_leader(spec).is_err(),
            "{spec} must be rejected as a leader"
        );
    }
}

#[test]
fn parse_leader_rejects_terminal_alias_chords() {
    // Ctrl+I == Tab and Ctrl+M == Enter at the byte level, so crossterm
    // never reports them as Char('i')/Char('m') and the leader would be
    // unrecognizable.
    assert!(parse_leader("ctrl+i").is_err(), "ctrl+i aliases Tab");
    assert!(parse_leader("ctrl+m").is_err(), "ctrl+m aliases Enter");
    // Neighboring letters remain valid.
    assert!(parse_leader("ctrl+j").is_ok());
    assert!(parse_leader("ctrl+n").is_ok());
}

#[test]
fn parse_leader_rejects_non_ctrl_and_multichar() {
    assert!(parse_leader("g").is_err(), "bare key is not a ctrl chord");
    assert!(parse_leader("ctrl+ab").is_err(), "leader is a single key");
}

#[test]
fn input_leader_parses_from_toml() {
    let toml = r#"
[input]
leader = "ctrl+a"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.input.leader, "ctrl+a");
    crate::config::validate_config(&cfg).unwrap();
    let leader = parse_leader(&cfg.input.leader).unwrap();
    assert_eq!(leader.code, KeyCode::Char('a'));
}

#[test]
fn parse_leader_accepts_uppercase_and_whitespace() {
    let leader = parse_leader("  CTRL+B  ").unwrap();
    assert_eq!(leader.code, KeyCode::Char('b'));
    assert!(leader.modifiers.contains(KeyModifiers::CONTROL));
}

#[test]
fn parse_leader_rejects_non_ctrl_chords() {
    assert!(parse_leader("b").is_err());
    assert!(parse_leader("alt+b").is_err());
    assert!(parse_leader("shift+b").is_err());
}

#[test]
fn parse_leader_rejects_reserved_and_multichar_keys() {
    // F-keys, named keys, and multi-char specs are not single ctrl+ascii
    // chords, so they fail the ctrl+ prefix / single-char gates.
    assert!(parse_leader("ctrl+f1").is_err());
    assert!(parse_leader("f1").is_err());
    assert!(parse_leader("ctrl+pageup").is_err());
    assert!(parse_leader("ctrl+").is_err());
    assert!(parse_leader("ctrl+ ").is_err());
}