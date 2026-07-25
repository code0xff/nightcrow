use crate::config::{
    Config, MAX_STARTUP_COMMANDS, StartupCommand, resolve_startup_commands, validate_config,
};

#[test]
fn startup_commands_default_to_empty() {
    let cfg = Config::default();
    assert!(cfg.startup_commands.is_empty());
    // A config without the table also defaults to empty and validates.
    let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
    assert!(cfg.startup_commands.is_empty());
    validate_config(&cfg).unwrap();
}

#[test]
fn startup_commands_parse_array_of_tables() {
    let toml = r#"
[[startup_command]]
name = "Claude"
command = "claude"

[[startup_command]]
command = "cargo test"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.startup_commands.len(), 2);
    assert_eq!(cfg.startup_commands[0].name.as_deref(), Some("Claude"));
    assert_eq!(cfg.startup_commands[0].command, "claude");
    assert_eq!(cfg.startup_commands[1].name, None);
    assert_eq!(cfg.startup_commands[1].command, "cargo test");
    validate_config(&cfg).unwrap();
}

#[test]
fn resolve_startup_commands_appends_cli_exec_after_config() {
    let mut cfg = Config::default();
    cfg.startup_commands.push(StartupCommand {
        name: Some("Claude".into()),
        command: "claude".into(),
    });
    let resolved =
        resolve_startup_commands(&cfg, &["codex".to_string(), "vim".to_string()]).unwrap();
    assert_eq!(resolved.len(), 3);
    assert_eq!(resolved[0].command, "claude");
    assert_eq!(resolved[0].name.as_deref(), Some("Claude"));
    // CLI entries carry no name and are ordered after config entries.
    assert_eq!(resolved[1].command, "codex");
    assert_eq!(resolved[1].name, None);
    assert_eq!(resolved[2].command, "vim");
}

#[test]
fn resolve_startup_commands_empty_when_nothing_configured() {
    let resolved = resolve_startup_commands(&Config::default(), &[]).unwrap();
    assert!(resolved.is_empty());
}

#[test]
fn resolve_startup_commands_rejects_empty_exec() {
    let resolved = resolve_startup_commands(&Config::default(), &["  ".to_string()]);
    assert!(resolved.is_err());
}

#[test]
fn resolve_startup_commands_caps_combined_total() {
    let mut cfg = Config::default();
    for i in 0..4 {
        cfg.startup_commands.push(StartupCommand {
            name: None,
            command: format!("echo {i}"),
        });
    }
    // 4 config + 5 CLI = 9 > MAX_STARTUP_COMMANDS (8).
    let cli: Vec<String> = (0..5).map(|i| format!("run {i}")).collect();
    assert!(resolve_startup_commands(&cfg, &cli).is_err());
    // 4 config + 4 CLI = 8 is exactly the cap.
    let cli: Vec<String> = (0..4).map(|i| format!("run {i}")).collect();
    assert_eq!(
        resolve_startup_commands(&cfg, &cli).unwrap().len(),
        MAX_STARTUP_COMMANDS
    );
}
