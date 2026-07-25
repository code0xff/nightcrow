use crate::config::{
    Config, EXAMPLE_CONFIG, InitOutcome, MAX_STARTUP_COMMANDS, StartupCommand, validate_config,
    write_config_template,
};

#[test]
fn default_config_is_valid() {
    validate_config(&Config::default()).unwrap();
}

#[test]
fn example_config_parses_and_validates() {
    // Guards the shipped config.example.toml against drift: it must parse
    // into Config and pass the same validation as a real user file. This is
    // the exact text `nightcrow init` writes, so the guard covers both.
    let cfg: Config = toml::from_str(EXAMPLE_CONFIG).expect("config.example.toml should parse");
    validate_config(&cfg).expect("config.example.toml should validate");
}

#[test]
fn write_config_template_creates_file_and_parent_dir() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("nested").join("config.toml");
    match write_config_template(&path, false).unwrap() {
        InitOutcome::Created(p) => assert_eq!(p, path),
        InitOutcome::AlreadyExists(_) => panic!("expected Created on a fresh path"),
    }
    let written = std::fs::read_to_string(&path).unwrap();
    assert_eq!(written, EXAMPLE_CONFIG);
}

#[test]
fn write_config_template_preserves_existing_without_force() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "# user edits\n").unwrap();
    match write_config_template(&path, false).unwrap() {
        InitOutcome::AlreadyExists(p) => assert_eq!(p, path),
        InitOutcome::Created(_) => panic!("must not overwrite an existing file"),
    }
    // The user's content survives untouched.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "# user edits\n");
}

#[test]
fn write_config_template_overwrites_with_force() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "# stale\n").unwrap();
    match write_config_template(&path, true).unwrap() {
        InitOutcome::Created(p) => assert_eq!(p, path),
        InitOutcome::AlreadyExists(_) => panic!("force should rewrite the file"),
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), EXAMPLE_CONFIG);
}

#[test]
fn parse_toml_overrides() {
    let toml = r#"
[layout]
upper_pct = 60
file_list_pct = 30
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.layout.upper_pct, 60);
    assert_eq!(cfg.layout.file_list_pct, 30);
}

#[test]
fn validation_rejects_out_of_range() {
    let mut cfg = Config::default();
    cfg.layout.upper_pct = 0;
    assert!(validate_config(&cfg).is_err());
    cfg.layout.upper_pct = 100;
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn startup_command_validation_rejects_empty_command() {
    let mut cfg = Config::default();
    cfg.startup_commands.push(StartupCommand {
        name: Some("blank".into()),
        command: "   ".into(),
    });
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn startup_command_validation_rejects_too_many() {
    let mut cfg = Config::default();
    for i in 0..(MAX_STARTUP_COMMANDS + 1) {
        cfg.startup_commands.push(StartupCommand {
            name: None,
            command: format!("echo {i}"),
        });
    }
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn startup_command_validation_accepts_max() {
    let mut cfg = Config::default();
    for i in 0..MAX_STARTUP_COMMANDS {
        cfg.startup_commands.push(StartupCommand {
            name: None,
            command: format!("echo {i}"),
        });
    }
    assert!(validate_config(&cfg).is_ok());
}

#[test]
fn validate_rejects_bad_leader() {
    let mut cfg = Config::default();
    cfg.input.leader = "f1".to_string();
    assert!(validate_config(&cfg).is_err());
}
