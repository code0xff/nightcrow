use crate::config::{Config, ShellConfig};

#[test]
fn default_shell_config_has_no_program() {
    let cfg = ShellConfig::default();
    assert!(cfg.program.is_none());
}

#[test]
fn default_shell_config_has_platform_command_args() {
    let cfg = ShellConfig::default();
    if cfg!(windows) {
        assert_eq!(cfg.command_args(), &["/C"]);
    } else {
        assert_eq!(cfg.command_args(), &["-lc"]);
    }
}

#[test]
fn resolved_program_uses_platform_default_when_none() {
    let cfg = ShellConfig::default();
    let program = cfg.resolved_program();
    if cfg!(windows) {
        // On Windows, %ComSpec% is usually set; if not, falls back to cmd.exe
        assert!(
            program == std::env::var("ComSpec").unwrap_or_else(|_| "cmd.exe".to_string()),
            "resolved program should be %ComSpec% or cmd.exe, got {program}"
        );
    } else {
        // On Unix, $SHELL is usually set; if not, falls back to /bin/sh
        assert!(
            program == std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string()),
            "resolved program should be $SHELL or /bin/sh, got {program}"
        );
    }
}

#[test]
fn explicit_program_overrides_platform_default() {
    let cfg = ShellConfig {
        program: Some("/usr/bin/zsh".to_string()),
        command_args: vec!["-lc".to_string()],
    };
    assert_eq!(cfg.resolved_program(), "/usr/bin/zsh");
}

#[test]
fn explicit_command_args_override_platform_default() {
    let cfg = ShellConfig {
        program: None,
        command_args: vec!["-c".to_string()],
    };
    assert_eq!(cfg.command_args(), &["-c"]);
}

#[test]
fn empty_command_args_is_allowed() {
    let cfg = ShellConfig {
        program: None,
        command_args: vec![],
    };
    assert!(cfg.command_args().is_empty());
}

#[test]
fn shell_section_absent_from_toml_uses_defaults() {
    let toml = r#"
[layout]
upper_pct = 60
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    // When [shell] is absent, program is None and command_args are platform defaults
    assert!(cfg.shell.program.is_none());
    if cfg!(windows) {
        assert_eq!(cfg.shell.command_args(), &["/C"]);
    } else {
        assert_eq!(cfg.shell.command_args(), &["-lc"]);
    }
}

#[test]
fn shell_section_with_explicit_program_and_args() {
    let toml = r#"
[shell]
program = "/opt/homebrew/bin/bash"
command_args = ["-lc"]
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.shell.program.as_deref(), Some("/opt/homebrew/bin/bash"));
    assert_eq!(cfg.shell.command_args(), &["-lc"]);
}

#[test]
fn shell_section_with_empty_command_args() {
    let toml = r#"
[shell]
program = "/bin/dash"
command_args = []
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.shell.program.as_deref(), Some("/bin/dash"));
    assert!(cfg.shell.command_args().is_empty());
}

#[test]
fn shell_section_with_missing_program_field() {
    let toml = r#"
[shell]
command_args = ["-c"]
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert!(cfg.shell.program.is_none());
    assert_eq!(cfg.shell.command_args(), &["-c"]);
}

#[test]
fn unix_default_equivalence() {
    // The default ShellConfig must resolve to exactly the same program and args
    // as the old hardcoded behaviour: $SHELL or /bin/sh, args ["-lc"].
    let cfg = ShellConfig::default();
    let expected_program = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let expected_args: &[String] = &["-lc".to_string()];

    if !cfg!(windows) {
        assert_eq!(cfg.resolved_program(), expected_program);
        assert_eq!(cfg.command_args(), expected_args);
    }
}
