use serde::{Deserialize, Serialize};

/// Startup behavior shared by every terminal surface in the session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalConfig {
    /// Open one bare shell when a project has no explicit startup commands.
    pub auto_open: bool,
}
