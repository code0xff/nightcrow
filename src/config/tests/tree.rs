use crate::config::{Config, TreeConfig, validate_config};

#[test]
fn tree_config_defaults_are_sane() {
    let cfg = TreeConfig::default();
    assert!(cfg.respect_gitignore);
    assert_eq!(cfg.max_depth, 64);
    assert!(cfg.live_watch, "live watching is on by default");
}

#[test]
fn tree_config_parses_from_toml() {
    let toml = r#"
[tree]
respect_gitignore = false
max_depth = 12
live_watch = false
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert!(!cfg.tree.respect_gitignore);
    assert_eq!(cfg.tree.max_depth, 12);
    assert!(!cfg.tree.live_watch);
    validate_config(&cfg).unwrap();
}

#[test]
fn tree_max_depth_validation_rejects_out_of_range() {
    let mut cfg = Config::default();
    cfg.tree.max_depth = 0;
    assert!(validate_config(&cfg).is_err());
    cfg.tree.max_depth = 1025;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn config_without_tree_table_defaults() {
    // A pre-existing config file with no [tree] table must still parse and
    // validate, falling back to defaults.
    let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
    assert!(cfg.tree.respect_gitignore);
    assert_eq!(cfg.tree.max_depth, 64);
    assert!(cfg.tree.live_watch);
    validate_config(&cfg).unwrap();
}
