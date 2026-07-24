use crate::config::{
    Config, LogConfig, LogLevel, LogRotation, validate_config,
};

#[test]
fn parse_rejects_invalid_log_rotation() {
    let toml = r#"
[log]
rotation = "weekly"
"#;
    assert!(toml::from_str::<Config>(toml).is_err());
}

#[test]
fn parse_rejects_invalid_log_level() {
    let toml = r#"
[log]
level = "verbose"
"#;
    assert!(toml::from_str::<Config>(toml).is_err());
}

#[test]
fn parse_accepts_all_valid_rotations() {
    for rotation in &["daily", "hourly", "size"] {
        let toml = format!("[log]\nrotation = \"{rotation}\"\n");
        assert!(
            toml::from_str::<Config>(&toml).is_ok(),
            "rotation={rotation} should parse"
        );
    }
}

#[test]
fn parse_accepts_all_valid_levels() {
    for level in &["error", "warn", "info", "debug", "trace"] {
        let toml = format!("[log]\nlevel = \"{level}\"\n");
        assert!(
            toml::from_str::<Config>(&toml).is_ok(),
            "level={level} should parse"
        );
    }
}

#[test]
fn log_config_defaults_are_sane() {
    let cfg = LogConfig::default();
    assert!(cfg.enabled);
    assert!(!cfg.prompt_log);
    assert_eq!(cfg.rotation, LogRotation::Daily);
    assert_eq!(cfg.level, LogLevel::Info);
    assert_eq!(cfg.max_days, 7);
    assert_eq!(cfg.commit_log_page_size, 100);
    assert_eq!(cfg.commit_log_prefetch_threshold, 25);
}

#[test]
fn commit_log_pagination_parses_from_toml() {
    let toml = r#"
[log]
commit_log_page_size = 400
commit_log_prefetch_threshold = 80
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.log.commit_log_page_size, 400);
    assert_eq!(cfg.log.commit_log_prefetch_threshold, 80);
    validate_config(&cfg).unwrap();
}

#[test]
fn commit_log_page_size_validation_rejects_out_of_range() {
    let mut cfg = Config::default();
    cfg.log.commit_log_page_size = 49;
    assert!(validate_config(&cfg).is_err());
    cfg.log.commit_log_page_size = 501;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn commit_log_prefetch_threshold_validation_rejects_zero() {
    let mut cfg = Config::default();
    cfg.log.commit_log_prefetch_threshold = 0;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn commit_log_prefetch_threshold_validation_rejects_above_page_size() {
    let mut cfg = Config::default();
    cfg.log.commit_log_page_size = 300;
    cfg.log.commit_log_prefetch_threshold = 301;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn log_max_size_mb_validation_rejects_zero_and_huge() {
    let mut cfg = Config::default();
    cfg.log.max_size_mb = 0;
    assert!(validate_config(&cfg).is_err());
    cfg.log.max_size_mb = 10_001;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn log_max_size_mb_validation_accepts_in_range() {
    let mut cfg = Config::default();
    cfg.log.max_size_mb = 1;
    assert!(validate_config(&cfg).is_ok());
    cfg.log.max_size_mb = 10_000;
    assert!(validate_config(&cfg).is_ok());
}

#[test]
fn log_max_days_validation_accepts_zero_as_keep_forever_sentinel() {
    let mut cfg = Config::default();
    cfg.log.max_days = 0;
    assert!(validate_config(&cfg).is_ok());
}

#[test]
fn log_max_days_validation_rejects_unreasonable_horizon() {
    let mut cfg = Config::default();
    cfg.log.max_days = 3651;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn log_config_parses_from_toml() {
    let toml = r#"
[log]
enabled = false
prompt_log = true
rotation = "size"
max_size_mb = 5
max_days = 14
level = "debug"
dir = "/tmp/logs"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert!(!cfg.log.enabled);
    assert!(cfg.log.prompt_log);
    assert_eq!(cfg.log.rotation, LogRotation::Size);
    assert_eq!(cfg.log.max_size_mb, 5);
    assert_eq!(cfg.log.max_days, 14);
    assert_eq!(cfg.log.level, LogLevel::Debug);
    assert_eq!(cfg.log.dir, "/tmp/logs");
}