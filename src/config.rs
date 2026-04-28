use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub layout: LayoutConfig,
    pub keys: KeybindingsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Percentage of vertical space for the upper (diff) panel (1–99)
    pub upper_pct: u16,
    /// Percentage of horizontal space for the file list within the upper panel (1–99)
    pub file_list_pct: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    pub quit: String,
    pub focus_toggle: String,
    pub new_pane: String,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            upper_pct: 55,
            file_list_pct: 25,
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            quit: "q".into(),
            focus_toggle: "Tab".into(),
            new_pane: "ctrl-t".into(),
        }
    }
}

pub fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("nightcrow").join("config.toml"))
}

pub fn load_config() -> Result<Config> {
    let path = match default_config_path() {
        Some(p) if p.exists() => p,
        _ => return Ok(Config::default()),
    };

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading config file {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&text).with_context(|| format!("parsing config file {}", path.display()))?;
    validate_config(&cfg)?;
    Ok(cfg)
}

fn validate_config(cfg: &Config) -> Result<()> {
    anyhow::ensure!(
        cfg.layout.upper_pct >= 1 && cfg.layout.upper_pct <= 99,
        "layout.upper_pct must be between 1 and 99"
    );
    anyhow::ensure!(
        cfg.layout.file_list_pct >= 1 && cfg.layout.file_list_pct <= 99,
        "layout.file_list_pct must be between 1 and 99"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        validate_config(&Config::default()).unwrap();
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
        assert_eq!(cfg.keys.quit, "q");
    }

    #[test]
    fn validation_rejects_out_of_range() {
        let mut cfg = Config::default();
        cfg.layout.upper_pct = 0;
        assert!(validate_config(&cfg).is_err());
        cfg.layout.upper_pct = 100;
        assert!(validate_config(&cfg).is_err());
    }
}
