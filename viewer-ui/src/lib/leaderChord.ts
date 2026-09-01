// The leader chord: how it is written down, how it is recognised, and what it
// sends to a pane when it is pressed twice.
//
// The TUI encodes the literal leader with `src/input/encode.rs::encode_key`;
// the web's terminal-side encoder for the same bytes is
// `viewer-ui/src/lib/termKeys.ts::ctrlSequence`. `literalLeaderSequence` below
// is the same formula applied to a leader chord rather than to a keystroke —
// `Ctrl+<letter>` is the letter's position in the alphabet, `Alt+<key>` takes
// the ESC (Meta) prefix — because the byte a person sees in their shell must
// not depend on which face of the session they typed it into.
//
// Nothing here touches the DOM: events arrive as the minimal `ShortcutKeyEvent`
// shape so the tests run in vitest's default `node` environment, exactly as
// `hardwareKeys.ts` does with `TypedKey`.

import type { ShortcutKeyEvent } from "./shortcutKeys";

export interface ChordSpec {
  /**
   * A `KeyboardEvent.key` value: one character for a printable key (stored
   * upper case, compared case-insensitively) or a named key such as
   * `"ArrowLeft"`.
   */
  key: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  meta: boolean;
}

type ModifierField = "ctrl" | "shift" | "alt" | "meta";

// Spellings a person may reasonably type for a modifier. Anything else in a
// modifier position is a typo, not a key name, and makes the whole chord
// invalid rather than being read as the chord's key.
const MODIFIER_ALIASES: Record<string, ModifierField> = {
  ctrl: "ctrl",
  control: "ctrl",
  alt: "alt",
  option: "alt",
  shift: "shift",
  meta: "meta",
  cmd: "meta",
  command: "meta",
  super: "meta",
  win: "meta",
};

// Named keys canonicalised so `parseChord` and `formatChord` round-trip
// whatever casing was typed. Values are the `KeyboardEvent.key` spellings.
const NAMED_KEYS = [
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Escape",
  "Enter",
  "Tab",
  "Space",
  "Backspace",
  "Delete",
  "Insert",
  "Home",
  "End",
  "PageUp",
  "PageDown",
  "F1",
  "F2",
  "F3",
  "F4",
  "F5",
  "F6",
  "F7",
  "F8",
  "F9",
  "F10",
  "F11",
  "F12",
];

const NAMED_BY_LOWER = new Map(NAMED_KEYS.map((name) => [name.toLowerCase(), name]));

function canonicalKeyName(raw: string): string {
  const named = NAMED_BY_LOWER.get(raw.toLowerCase());
  if (named) return named;
  return raw.length === 1 ? raw.toUpperCase() : raw;
}

/** A space arrives as `" "` on the wire but reads as `Space` in a chord. */
function normalizeEventKey(key: string): string {
  return key === " " ? "Space" : key;
}

/**
 * Parse a chord written as `Ctrl+Shift+ArrowLeft`, or null when the text is not
 * a chord: empty, modifier-only, an unknown or repeated modifier, or more than
 * one non-modifier key.
 */
export function parseChord(text: string): ChordSpec | null {
  const parts = text.split("+").map((part) => part.trim());
  if (parts.some((part) => part.length === 0)) return null;

  const spec: ChordSpec = { key: "", ctrl: false, shift: false, alt: false, meta: false };
  const keys: string[] = [];
  for (const part of parts) {
    const modifier = MODIFIER_ALIASES[part.toLowerCase()];
    if (modifier) {
      if (spec[modifier]) return null;
      spec[modifier] = true;
      continue;
    }
    keys.push(part);
  }
  if (keys.length !== 1) return null;
  spec.key = canonicalKeyName(keys[0]);
  return spec;
}

/** The canonical display form: `Ctrl+Alt+Shift+Meta+Key`. */
export function formatChord(spec: ChordSpec): string {
  const parts: string[] = [];
  if (spec.ctrl) parts.push("Ctrl");
  if (spec.alt) parts.push("Alt");
  if (spec.shift) parts.push("Shift");
  if (spec.meta) parts.push("Meta");
  parts.push(canonicalKeyName(spec.key));
  return parts.join("+");
}

/**
 * Whether an event is exactly this chord.
 *
 * Modifiers must be equal, not merely present: a chord that matched supersets
 * would let `Ctrl+Shift+Meta+ArrowLeft` fire the `Ctrl+Shift+ArrowLeft` command
 * and steal a chord the OS or another binding owns. Same exact-equality rule the
 * TUI applies in `src/input/routing.rs::map_key`.
 */
export function chordMatches(spec: ChordSpec, event: ShortcutKeyEvent): boolean {
  if (event.ctrlKey !== spec.ctrl) return false;
  if (event.shiftKey !== spec.shift) return false;
  if (event.altKey !== spec.alt) return false;
  if (event.metaKey !== spec.meta) return false;
  return normalizeEventKey(event.key).toLowerCase() === spec.key.toLowerCase();
}

/** `Ctrl+F`, the same default the TUI ships (`[input] leader`). */
export const DEFAULT_LEADER: ChordSpec = {
  key: "F",
  ctrl: true,
  shift: false,
  alt: false,
  meta: false,
};

// Single-modifier chords the browser or OS already spends on something the user
// will miss. Claiming one still works — the handler calls preventDefault — but
// the settings UI has to say what is being taken away.
const RESERVED_LETTERS: Record<string, string> = {
  F: "the browser's in-page Find",
  T: "opening a new tab",
  N: "opening a new window",
  W: "closing the tab",
  L: "focusing the address bar",
  D: "bookmarking the page",
  P: "printing",
  S: "saving the page",
  R: "reloading the page",
};

// Ctrl+Shift+I/J/C are the default devtools chords in Chrome, Edge and Firefox.
const DEVTOOLS_LETTERS: Record<string, string> = {
  I: "opening developer tools",
  J: "opening the developer console",
  C: "the devtools element inspector",
};

/**
 * A human-readable warning when a chord collides with a known browser or OS
 * shortcut, else null. Advisory: the caller decides whether to allow it.
 */
export function leaderConflict(spec: ChordSpec): string | null {
  const key = canonicalKeyName(spec.key);
  const primary = spec.ctrl || spec.meta;

  if (primary && !spec.alt && key === "Space") {
    return `${formatChord(spec)} switches the input source on most systems, so an IME user may never reach it.`;
  }
  if (spec.ctrl && spec.shift && !spec.alt && !spec.meta) {
    const devtools = DEVTOOLS_LETTERS[key];
    if (devtools) {
      return `${formatChord(spec)} is the browser default for ${devtools}.`;
    }
  }
  if (primary && !spec.shift && !spec.alt) {
    const reserved = RESERVED_LETTERS[key];
    if (reserved) {
      return `${formatChord(spec)} is the browser shortcut for ${reserved}.`;
    }
  }
  return null;
}

/**
 * The bytes to send to the focused pane when the leader is pressed twice, so
 * the web reproduces the TUI's literal-leader passthrough, or null for a chord
 * a terminal has no encoding for (a Meta chord, or a named key with a
 * modifier). See the module header for where the same formula lives on the
 * other two sides.
 */
export function literalLeaderSequence(spec: ChordSpec): string | null {
  if (spec.meta) return null;
  // `Space` is the one named key with a character behind it, and a terminal has
  // a byte for it (`Ctrl+Space` is NUL), so it is folded back to the character.
  const key = spec.key === "Space" ? " " : spec.key;
  if (key.length !== 1) return null;
  const typed = spec.shift ? key.toUpperCase() : key.toLowerCase();

  if (spec.ctrl) {
    const control = controlByte(typed);
    if (control === null) return null;
    // Ctrl+Alt is ESC + the control byte, matching `encode_key`.
    return spec.alt ? "\x1b" + control : control;
  }
  if (spec.alt) return "\x1b" + typed;
  return typed;
}

/**
 * The C0 control character for a Ctrl chord: the character's place in the ASCII
 * table with the top bits cleared. Same table as
 * `termKeys.ts::ctrlSequence`, kept here so a chord spec does not have to be
 * turned into typed input first.
 */
function controlByte(typed: string): string | null {
  if (typed === " ") return "\0";
  if (typed === "?") return "\x7f";
  let code = typed.charCodeAt(0);
  if (code >= 0x61 && code <= 0x7a) code -= 0x20;
  if (code < 0x40 || code > 0x5f) return null;
  return String.fromCharCode(code & 0x1f);
}
