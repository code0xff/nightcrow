use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentIndicatorConfig {
    pub enabled: bool,
    /// Seconds within which a file is considered hot after its mtime.
    /// Must be >= 3 so the bright→normal fade transition stays observable.
    pub hot_window_secs: u64,
    /// When idle (no manual navigation for >=2s), move selection to the
    /// freshest hot file. Opt-in: the file list follows whichever file was
    /// most recently touched on disk — useful when an agent or external
    /// process is editing files in a neighbouring pane.
    pub auto_follow: bool,
}

impl Default for AgentIndicatorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            hot_window_secs: 15,
            auto_follow: false,
        }
    }
}

/// Configuration for the read-only file-tree navigator (`ViewMode::Tree`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TreeConfig {
    /// Hide paths matched by `.gitignore`. On by default so the tree doesn't
    /// explode into build artifacts.
    pub respect_gitignore: bool,
    /// Maximum directory depth the navigator will expand into. Guard against
    /// pathologically deep trees; must be in 1..=1024.
    pub max_depth: usize,
    /// Watch expanded directories for filesystem changes and refresh the tree
    /// live while Tree mode is open. Only visible (expanded) directories are
    /// watched, non-recursively. Set to `false` on very large trees or
    /// filesystems where native watching is costly or unsupported.
    pub live_watch: bool,
}

impl Default for TreeConfig {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            max_depth: 64,
            live_watch: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MouseConfig {
    /// Capture the mouse so clicks reach mouse-aware pane programs and wheel
    /// scrolls the pane under the pointer. While captured, the outer terminal
    /// only does its own text selection with Shift held. Set to `false` to
    /// give the mouse back to the outer terminal entirely.
    pub enabled: bool,
}

impl Default for MouseConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}
