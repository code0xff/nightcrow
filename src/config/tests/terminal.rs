use crate::config::Config;

#[test]
fn terminal_does_not_auto_open_by_default() {
    assert!(!Config::default().terminal.auto_open);
    let parsed: Config = toml::from_str("").unwrap();
    assert!(!parsed.terminal.auto_open);
}

#[test]
fn terminal_auto_open_can_be_enabled() {
    let parsed: Config = toml::from_str("[terminal]\nauto_open = true\n").unwrap();
    assert!(parsed.terminal.auto_open);
}
