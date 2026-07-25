use crate::config::{AgentIndicatorConfig, Config, validate_config};

#[test]
fn agent_indicator_defaults_are_sane() {
    let cfg = AgentIndicatorConfig::default();
    assert!(cfg.enabled);
    assert!(!cfg.auto_follow);
    assert_eq!(cfg.hot_window_secs, 15);
}

#[test]
fn agent_indicator_parses_from_toml() {
    let toml = r#"
[agent_indicator]
enabled = false
hot_window_secs = 30
auto_follow = false
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert!(!cfg.agent_indicator.enabled);
    assert!(!cfg.agent_indicator.auto_follow);
    assert_eq!(cfg.agent_indicator.hot_window_secs, 30);
}

#[test]
fn mouse_capture_defaults_on_and_parses_from_toml() {
    assert!(Config::default().mouse.enabled);

    let cfg: Config = toml::from_str("[mouse]\nenabled = false\n").unwrap();
    assert!(!cfg.mouse.enabled);
}

#[test]
fn agent_indicator_validation_rejects_too_small_window() {
    let mut cfg = Config::default();
    cfg.agent_indicator.hot_window_secs = 2;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn agent_indicator_validation_rejects_too_large_window() {
    let mut cfg = Config::default();
    cfg.agent_indicator.hot_window_secs = 3601;
    assert!(validate_config(&cfg).is_err());
}
