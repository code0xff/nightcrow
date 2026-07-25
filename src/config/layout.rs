use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

/// Default leader chord. `Ctrl+F` is a one-handed left-hand chord that avoids
/// tmux's `Ctrl+B` (so nightcrow can run inside tmux) AND the Ctrl chords an
/// inner Claude Code pane reserves (`Ctrl+G` = external editor, plus
/// `Ctrl+O/R/S/T/L/…`). It also dodges terminal flow control (`Ctrl+Q`/`Ctrl+S`
/// = XON/XOFF) and the shell signals `Ctrl+C/D/Z`. Its only collision is
/// `Ctrl+F` as forward-char (readline) / page-forward (vim), which users
/// almost always reach via the arrow keys / PageDown instead; when needed it
/// stays reachable via `<leader><leader>`.
pub(super) const DEFAULT_LEADER: &str = "ctrl+f";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LayoutConfig {
    /// Percentage of vertical space for the upper (diff) panel (1–99)
    pub upper_pct: u16,
    /// Percentage of horizontal space for the file list within the upper panel (1–99)
    pub file_list_pct: u16,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            upper_pct: 55,
            file_list_pct: 25,
        }
    }
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
// empty, or `from_index` would rely on a runtime fallback we'd rather not
// exercise. `const` items don't accept `_` inside an `impl` block, so this
// lives at module scope.
const _: () = assert!(!Accent::ALL.is_empty(), "Accent::ALL must be non-empty");

impl Accent {
    // Variant declaration order MUST match this slice so accent_idx values
    // persisted in pre-existing session.json files keep mapping to the same
    // color after the strong-enum migration.
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
        // unreachable, but a runtime panic on a UI helper is worse than a
        // silently miscoloured tile. The roundtrip test pins the invariant.
        Self::ALL.iter().position(|&a| a == self).unwrap_or(0)
    }

    pub fn from_index(idx: usize) -> Accent {
        // The compile-time guard above keeps `len > 0`, so `% len` is sound.
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
    /// this key, then a follow-up (tmux-style). Accepts a single
    /// `ctrl+<ascii>` chord; the parser rejects anything that doubles as a
    /// no-prefix reserved key (F1..F10, Shift+arrows, Shift+PgUp/PgDn).
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
/// Only `ctrl+<ascii-printable>` chords are accepted. The chord must be a key
/// that `encode_key` can turn into literal bytes (so `<L><L>` can pass the
/// leader through to the PTY) and must NOT collide with a no-prefix reserved
/// key. F-keys, Shift+arrows, and Shift+PgUp/PgDn are reserved and rejected.
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
    // `encode_key` maps Ctrl+A..Ctrl+Z to control bytes 1..26. Digits and
    // punctuation (e.g. ctrl+1) have no single-control-byte encoding, so
    // encode_key would send the literal char instead and the pass-through
    // would break — hence they are rejected above.
    Ok(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

/// A single reserved startup command. `name` labels the pane's tab; when
/// absent the command text is used as the label.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StartupCommand {
    /// Optional tab label. Falls back to `command` when omitted.
    pub name: Option<String>,
    /// Shell command run in the pane immediately on launch.
    pub command: String,
}