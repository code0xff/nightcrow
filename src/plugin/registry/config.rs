use std::path::Path;

use super::PluginStatus;

/// What the loaded config says about an installed plugin.
pub fn status(cfg: &crate::config::Config, name: &str) -> PluginStatus {
    let declared = cfg.plugins.iter().find(|plugin| plugin.name == name);
    PluginStatus {
        declared: declared.is_some(),
        enabled: declared.is_some_and(|plugin| plugin.enabled),
        opt_ins: cfg
            .startup_commands
            .iter()
            .filter(|command| command.plugin.as_deref() == Some(name))
            .count(),
    }
}

/// A disabled `[[plugin]]` block for the user to review and paste.
pub fn config_snippet(name: &str, command: &Path) -> String {
    let name = toml::Value::String(name.to_string());
    let command = toml::Value::String(command.display().to_string());
    format!(
        "[[plugin]]\n\
         name = {name}\n\
         command = {command}\n\
         args = []\n\
         allowed_resume_flags = []\n\
         enabled = false\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_snippet_uses_toml_string_encoding() {
        let name = "watcher\nquoted\"";
        let command = Path::new("plugins/line\nbreak\\\"watcher");
        let snippet = config_snippet(name, command);
        let parsed: toml::Value = toml::from_str(&snippet).expect("valid TOML");

        assert_eq!(parsed["plugin"][0]["name"].as_str(), Some(name));
        assert_eq!(
            parsed["plugin"][0]["command"].as_str(),
            Some(command.to_string_lossy().as_ref())
        );
    }
}
