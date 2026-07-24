// The on-screen key bar for the terminal on touch devices. A phone's soft
// keyboard cannot produce Escape, Tab, the Ctrl combinations, or the arrow keys,
// yet those are exactly the keys an interactive shell needs — without Ctrl-C you
// cannot interrupt a runaway process, and without Escape you cannot leave vim.
// Each key here maps to the raw byte sequence a real keyboard would send, which
// the panel forwards to the PTY as ordinary input (`{type:"input", …}`), so the
// shell cannot tell a bar tap from a keystroke.
//
// Only keys the soft keyboard omits are listed — ordinary characters (`/`, `|`,
// `~`) stay off the bar, which is why it holds control/escape/arrows alone.

export type TermKey =
  | "esc"
  | "tab"
  | "ctrl-c"
  | "ctrl-d"
  | "ctrl-z"
  | "ctrl-l"
  | "ctrl-r"
  | "up"
  | "down"
  | "left"
  | "right";

// Control bytes are the letter's position in the alphabet: Ctrl-C is 0x03 (C is
// the 3rd letter), Ctrl-L 0x0c, and so on. Arrows here are the *normal*-mode CSI
// cursor sequences (`ESC [ A`…), matching what xterm.js emits with the default
// cursor-key mode.
export const TERM_KEY_SEQUENCES: Record<TermKey, string> = {
  esc: "\x1b",
  tab: "\t",
  "ctrl-c": "\x03",
  "ctrl-d": "\x04",
  "ctrl-z": "\x1a",
  "ctrl-l": "\x0c",
  "ctrl-r": "\x12",
  up: "\x1b[A",
  down: "\x1b[B",
  right: "\x1b[C",
  left: "\x1b[D",
};

// When an application turns on cursor-key (DECCKM / application cursor) mode —
// vim, less, and most full-screen TUIs do — the arrows must switch from CSI
// (`ESC [`) to SS3 (`ESC O`), or the app sees the wrong bytes and the cursor
// does not move. Only the four arrows differ between the modes; every other key
// is mode-independent. A real keyboard through xterm.js makes exactly this
// switch, so the bar has to as well.
const TERM_ARROW_APPLICATION: Partial<Record<TermKey, string>> = {
  up: "\x1bOA",
  down: "\x1bOB",
  right: "\x1bOC",
  left: "\x1bOD",
};

/**
 * The bytes a bar key sends to the PTY. `applicationCursor` is the active
 * terminal's cursor-key mode (xterm's `modes.applicationCursorKeysMode`); when
 * set, the arrows send their SS3 form so vim and friends read them correctly.
 */
export function termKeySequence(key: TermKey, applicationCursor = false): string {
  if (applicationCursor) {
    const ss3 = TERM_ARROW_APPLICATION[key];
    if (ss3) return ss3;
  }
  return TERM_KEY_SEQUENCES[key];
}

// The bar's layout, left to right: the two bare keys first, then the common Ctrl
// combinations, then the arrow cluster. `label` is what the button shows;
// `aria` names it for assistive tech, where "^C" would be read as a caret.
export const TERM_KEY_BAR: { key: TermKey; label: string; aria: string }[] = [
  { key: "esc", label: "Esc", aria: "Escape" },
  { key: "tab", label: "Tab", aria: "Tab" },
  { key: "ctrl-c", label: "^C", aria: "Control C" },
  { key: "ctrl-d", label: "^D", aria: "Control D" },
  { key: "ctrl-z", label: "^Z", aria: "Control Z" },
  { key: "ctrl-l", label: "^L", aria: "Control L" },
  { key: "ctrl-r", label: "^R", aria: "Control R" },
  { key: "left", label: "←", aria: "Left arrow" },
  { key: "down", label: "↓", aria: "Down arrow" },
  { key: "up", label: "↑", aria: "Up arrow" },
  { key: "right", label: "→", aria: "Right arrow" },
];
