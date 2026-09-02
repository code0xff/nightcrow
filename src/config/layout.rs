use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

/// Default leader chord. `Ctrl+F` avoids tmux's `Ctrl+B` (so nightcrow can
/// run inside tmux), the Ctrl chords an inner Claude Code pane reserves,
/// terminal flow control (`Ctrl+Q`/`Ctrl+S`), and shell signals
/// (`Ctrl+C/D/Z`). Its only collision is `Ctrl+F` as forward-char /
/// page-forward, which users almost always reach via arrow keys / PageDown;
/// when needed it stays reachable via `<leader><leader>`.
pub(super) const DEFAULT_LEADER: &str = "ctrl+f";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Percentage of vertical space for the upper (diff) panel (1–99)
    pub upper_pct: u16,
    /// Percentage of horizontal space for the file list within the upper panel (1–99)
    pub file_list_pct: u16,
    /// Where the project tab strip is drawn.
    pub tabs: TabStrip,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            upper_pct: 55,
            file_list_pct: 25,
            tabs: TabStrip::Top,
        }
    }
}

/// Where the project tabs sit. `Left` gives a wide, short terminal its rows
/// back at the cost of a fixed column of width; read when the TUI attaches,
/// like the rest of `[layout]`, so panes are never resized by a live flip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabStrip {
    /// One row across the top of the screen.
    #[default]
    Top,
    /// A column down the left of the body, one tab per row.
    Left,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    #[default]
    Yellow,
    Cyan,
    Green,
    Magenta,
    Blue,
}

// Compile-time guard: a future refactor must not shrink `Accent::ALL` to
// empty, or `from_index` would rely on a runtime fallback.
const _: () = assert!(!Accent::ALL.is_empty(), "Accent::ALL must be non-empty");

impl Accent {
    // Variant declaration order MUST match this slice so accent indices already
    // written down keep mapping to the same color.
    pub const ALL: &'static [Accent] = &[
        Accent::Yellow,
        Accent::Cyan,
        Accent::Green,
        Accent::Magenta,
        Accent::Blue,
    ];

    pub fn color(self) -> ratatui::style::Color {
        use ratatui::style::Color::*;
        match self {
            Accent::Yellow => Yellow,
            Accent::Green => Green,
            Accent::Cyan => Cyan,
            Accent::Magenta => Magenta,
            Accent::Blue => Blue,
        }
    }

    pub fn index(self) -> usize {
        // Fall back to 0 when a variant is missing from `ALL` — should be
        // unreachable, but a runtime panic on a UI helper is worse.
        Self::ALL.iter().position(|&a| a == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Accent {
        Self::ALL
            .get(idx % Self::ALL.len())
            .copied()
            .unwrap_or(Accent::Yellow)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Accent color preset.
    pub name: Accent,
}

impl ThemeConfig {
    pub fn preset_index(&self) -> usize {
        self.name.index()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InputConfig {
    /// The leader (prefix) chord. Every app command is reached by pressing
    /// this key, then a follow-up (tmux-style).
    pub leader: String,
}

impl Default for InputConfig {
    fn default() -> Self {
        Self {
            leader: DEFAULT_LEADER.to_string(),
        }
    }
}

/// Parse a leader chord string (e.g. `"ctrl+b"`) into a `KeyEvent`.
///
/// Only `ctrl+<ascii-letter>` chords are accepted. The chord must be a key
/// `encode_key` can turn into literal bytes (so `<L><L>` can pass the leader
/// through to the PTY) and must not collide with a reserved key; F-keys,
/// Shift+arrows, and Shift+PgUp/PgDn are reserved and rejected.
pub fn parse_leader(spec: &str) -> Result<KeyEvent> {
    let normalized = spec.trim().to_ascii_lowercase();
    let rest = normalized.strip_prefix("ctrl+").ok_or_else(|| {
        anyhow::anyhow!(
            "input.leader \"{spec}\" must be a ctrl chord like \"ctrl+b\" \
             (only ctrl+<letter> leaders are supported)"
        )
    })?;
    let mut chars = rest.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        anyhow::ensure!(
            false,
            "input.leader \"{spec}\" must name exactly one ascii character after ctrl+"
        );
        unreachable!()
    };
    anyhow::ensure!(
        c.is_ascii_alphabetic(),
        "input.leader \"{spec}\" must use an ascii letter after ctrl+ \
         (e.g. ctrl+b; ctrl+1, ctrl+-, ctrl+space are not allowed)"
    );
    // Terminals send Ctrl+I as Tab and Ctrl+M as Enter, so crossterm surfaces
    // those as KeyCode::Tab / KeyCode::Enter — never the Char + CONTROL event
    // is_leader_key looks for. Such a leader could be armed but never
    // recognized, so reject it up front.
    anyhow::ensure!(
        !matches!(c, 'i' | 'm'),
        "input.leader \"{spec}\" is not usable: terminals deliver Ctrl+I as Tab \
         and Ctrl+M as Enter, so this leader would never be recognized"
    );
    // Restricting to letters guarantees `<L><L>` literal pass-through works:
    // `encode_key` maps Ctrl+A..Ctrl+Z to control bytes 1..26, while digits
    // and punctuation have no single-control-byte encoding, so the
    // pass-through would break — hence they are rejected above.
    Ok(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

/// A single reserved startup command. `name` labels the pane's tab; when
/// absent the command text is used as the label.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupCommand {
    /// Optional tab label. Falls back to `command` when omitted.
    pub name: Option<String>,
    /// Shell command run in the pane immediately on launch.
    pub command: String,
    /// Name of the `[[plugin]]` that may act on this pane. `None` — the default —
    /// means no plugin ever receives this pane's events or can act on it.
    #[serde(default)]
    pub plugin: Option<String>,
}
