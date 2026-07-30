use crate::config::{Config, MAX_PLUGINS, PluginConfig, StartupCommand, validate_config};

fn plugin(name: &str, enabled: bool) -> PluginConfig {
    PluginConfig {
        name: name.to_string(),
        command: "nightcrow-recovery".to_string(),
        enabled,
        ..PluginConfig::default()
    }
}

fn pane_using(plugin: Option<&str>) -> StartupCommand {
    StartupCommand {
        name: None,
        command: "claude".to_string(),
        plugin: plugin.map(str::to_string),
    }
}

#[test]
fn plugin_section_parses_and_stays_disabled_unless_enabled_is_set() {
    let toml = r#"
[[plugin]]
name = "recovery"
command = "nightcrow-recovery"
args = ["--verbose"]

[plugin.env]
NIGHTCROW_RECOVERY_LOG = "info"
"#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.plugins.len(), 1);
    assert_eq!(cfg.plugins[0].name, "recovery");
    assert_eq!(cfg.plugins[0].command, "nightcrow-recovery");
    assert_eq!(cfg.plugins[0].args, vec!["--verbose".to_string()]);
    assert_eq!(
        cfg.plugins[0].env.get("NIGHTCROW_RECOVERY_LOG").unwrap(),
        "info"
    );
    assert!(!cfg.plugins[0].enabled, "plugins must be off by default");
    validate_config(&cfg).unwrap();
}

#[test]
fn plugin_args_and_env_default_to_empty_when_omitted() {
    let cfg: Config =
        toml::from_str("[[plugin]]\nname = \"recovery\"\ncommand = \"run-me\"\n").unwrap();
    assert!(cfg.plugins[0].args.is_empty());
    assert!(cfg.plugins[0].env.is_empty());
}

#[test]
fn watch_on_signal_is_off_unless_the_config_asks_for_it() {
    // The default is what every existing config gets, and it has to keep meaning
    // "the opt-in list is the whole of what this plugin can see".
    let cfg: Config =
        toml::from_str("[[plugin]]\nname = \"recovery\"\ncommand = \"run-me\"\n").unwrap();
    assert!(!cfg.plugins[0].watch_on_signal);

    let cfg: Config = toml::from_str(
        "[[plugin]]\nname = \"recovery\"\ncommand = \"run-me\"\nwatch_on_signal = true\n",
    )
    .unwrap();
    assert!(cfg.plugins[0].watch_on_signal);
    validate_config(&cfg).unwrap();
}

#[test]
fn a_plugin_that_watches_on_signal_needs_no_pane_to_name_it() {
    // Its panes are the ones that will speak to it, and none of them can be known
    // in advance — so requiring an opt-in here would make the switch unusable.
    let cfg: Config = toml::from_str(
        "[[plugin]]\nname = \"recovery\"\ncommand = \"run-me\"\n\
         enabled = true\nwatch_on_signal = true\n",
    )
    .unwrap();
    validate_config(&cfg).unwrap();
}

#[test]
fn a_plugin_without_name_or_command_fails_to_deserialize() {
    assert!(toml::from_str::<Config>("[[plugin]]\ncommand = \"run-me\"\n").is_err());
    assert!(toml::from_str::<Config>("[[plugin]]\nname = \"recovery\"\n").is_err());
}

#[test]
fn absent_plugin_section_defaults_to_empty_and_validates() {
    assert!(Config::default().plugins.is_empty());
    let cfg: Config = toml::from_str("[layout]\nupper_pct = 50\n").unwrap();
    assert!(cfg.plugins.is_empty());
    validate_config(&cfg).unwrap();
}

#[test]
fn a_startup_command_without_a_plugin_field_defaults_to_no_plugin() {
    let cfg: Config = toml::from_str("[[startup_command]]\ncommand = \"claude\"\n").unwrap();
    assert_eq!(cfg.startup_commands[0].plugin, None);
    validate_config(&cfg).unwrap();
}

#[test]
fn a_startup_command_naming_an_enabled_plugin_validates() {
    let mut cfg = Config::default();
    cfg.plugins.push(plugin("recovery", true));
    cfg.startup_commands.push(pane_using(Some("recovery")));
    validate_config(&cfg).unwrap();
}

#[test]
fn a_startup_command_naming_an_unknown_plugin_is_rejected() {
    let mut cfg = Config::default();
    cfg.plugins.push(plugin("recovery", true));
    cfg.startup_commands.push(pane_using(Some("typo")));
    let err = validate_config(&cfg).unwrap_err().to_string();
    assert!(err.contains("typo"), "error should name the opt-in: {err}");
}

#[test]
fn a_startup_command_naming_a_disabled_plugin_still_validates() {
    // `enabled = false` is how a plugin gets switched off; requiring every
    // opt-in to be unpicked first would make that a two-place edit. Off simply
    // means the pane is never handed to anyone.
    let mut cfg = Config::default();
    cfg.plugins.push(plugin("recovery", false));
    cfg.startup_commands.push(pane_using(Some("recovery")));
    validate_config(&cfg).expect("a disabled plugin is a valid target");
}

#[test]
fn duplicate_plugin_names_are_rejected() {
    let mut cfg = Config::default();
    cfg.plugins.push(plugin("recovery", true));
    cfg.plugins.push(plugin("recovery", true));
    let err = validate_config(&cfg).unwrap_err().to_string();
    assert!(
        err.contains("recovery"),
        "error should name the clash: {err}"
    );
}

#[test]
fn a_blank_plugin_name_is_rejected() {
    let mut cfg = Config::default();
    cfg.plugins.push(plugin("   ", true));
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn a_blank_plugin_command_is_rejected() {
    let mut cfg = Config::default();
    let mut p = plugin("recovery", true);
    p.command = "  ".to_string();
    cfg.plugins.push(p);
    assert!(validate_config(&cfg).is_err());
}

#[test]
fn exceeding_the_plugin_cap_is_rejected_but_the_cap_itself_validates() {
    let mut cfg = Config::default();
    for i in 0..MAX_PLUGINS {
        cfg.plugins.push(plugin(&format!("p{i}"), true));
    }
    validate_config(&cfg).unwrap();
    cfg.plugins.push(plugin("one-too-many", true));
    assert!(validate_config(&cfg).is_err());
}
