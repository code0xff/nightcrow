// The on-screen key bar for the terminal on touch devices. A soft keyboard
// cannot produce Escape, Tab, the Ctrl combinations, or the arrow keys,
// yet those are exactly the keys an interactive shell needs — without Ctrl-C you
// cannot interrupt a runaway process, and without Escape you cannot leave vim.
// Each key here maps to the raw byte sequence a real keyboard would send, which
// the panel forwards to the PTY as ordinary input (`{type:"input", …}`), so the
// shell cannot tell a bar tap from a keystroke.
//
// Only keys the soft keyboard omits are listed — ordinary characters (`/`, `|`,
// `~`) stay off the bar, which is why it holds control/escape/arrows alone.
// Ctrl is there as well, but as a latch rather than a key: a shell uses more
// combinations than there are buttons for, so it arms and the next character
// typed is sent as the control byte instead (`ctrlSequence`, `useCtrlLatch`).

export type TermKey =
  | "esc"
  | "tab"
  | "shift-tab"
  | "ctrl-b"
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
// cursor-key mode. Shift-Tab has no control byte of its own — it is CSI Z
// (back-tab), the same escape a real keyboard's Shift-Tab produces.
export const TERM_KEY_SEQUENCES: Record<TermKey, string> = {
  esc: "\x1b",
  tab: "\t",
  "shift-tab": "\x1b[Z",
  "ctrl-b": "\x02",
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

/**
 * The bytes Ctrl held down with `typed` sends, or null for a character Ctrl has
 * no form for.
 *
 * A control byte is the character's place in the ASCII table with the top bits
 * cleared: `@` through `_` (0x40–0x5f) become 0x00–0x1f, which is where the
 * letters land once a lower-case one is raised. The two the block leaves out are
 * two a terminal still answers — Ctrl-Space is NUL (emacs' set-mark, tmux'
 * begin-selection) and Ctrl-? is DEL. The case is folded by code point rather
 * than `toUpperCase`, which for a handful of letters returns two characters and
 * would make a control byte out of the first.
 *
 * Everything else — a Hangul syllable, an emoji, a paste, an escape sequence
 * from a hardware key — is not something Ctrl combines with, and the caller
 * sends it as it was typed.
 */
export function ctrlSequence(typed: string): string | null {
  if (typed.length !== 1) return null;
  if (typed === " ") return "\0";
  if (typed === "?") return "\x7f";
  let code = typed.charCodeAt(0);
  if (code >= 0x61 && code <= 0x7a) code -= 0x20;
  if (code < 0x40 || code > 0x5f) return null;
  return String.fromCharCode(code & 0x1f);
}

/** What the latch does with one piece of terminal input. */
export interface CtrlLatchStep {
  /** The bytes to forward, modified or as they arrived. */
  data: string;
  /** Whether the latch is still armed afterwards. */
  armed: boolean;
}

/**
 * One step of "the armed Ctrl modifies the next thing typed".
 *
 * Escape-led input leaves the latch armed. It reaches the same handler but
 * comes from the program rather than from a person — a pane running tmux or
 * vim has focus reporting on, so merely putting the keyboard back in it emits
 * `ESC [ I`, and a mouse-tracking program reports every tap. The rule is the
 * whole escape-led prefix rather than the two reports it is here for, because
 * each automatic reply missed from a narrower list would disarm the latch with
 * nothing to show for it. What that costs is the other direction: a bracketed
 * paste (`ESC [ 2 0 0 ~`) and a hardware Escape or arrow leave it armed when a
 * person might have expected them to spend it. That way round is the one to be
 * wrong in — the button stays lit, where a latch that died quietly is only
 * discovered by the character it failed to modify.
 *
 * Everything else spends it, whether or not Ctrl has a byte for it: the person
 * typed, and if what they typed has no control form the mistake was the latch.
 * An unbracketed paste is spent this way too — xterm hands it over as ordinary
 * data, so a one-character paste is indistinguishable from typing that
 * character.
 */
export function ctrlLatchStep(armed: boolean, typed: string): CtrlLatchStep {
  if (!armed) return { data: typed, armed: false };
  if (typed.startsWith("\x1b")) return { data: typed, armed: true };
  return { data: ctrlSequence(typed) ?? typed, armed: false };
}

/** Tailwind's `md`. A window this narrow belongs to a phone whatever it reports
 *  about its pointer, so the bar defaults on below it either way. */
export const KEYBOARD_MIN_VIEWPORT_PX = 768;

export type KeyBarPref = "shown" | "hidden";

/** What was stored, or null for anything this version does not recognise. */
export function parseKeyBarPref(raw: string | null): KeyBarPref | null {
  return raw === "shown" || raw === "hidden" ? raw : null;
}

/**
 * Whether to show the bar on a screen nobody has chosen for yet.
 *
 * A coarse pointer is the question that actually matters — "is what types here
 * a pane of glass" — and it is the one a tablet answers differently from the
 * desktop it is as wide as. Width alone would have left an iPad, which is
 * wider than the `md` the bar used to hide at, with no Escape and no Ctrl-C.
 * Width still decides for anything a pointer cannot: a phone in desktop mode,
 * a browser that reports nothing. Same question the terminal font asks
 * (`termFont.ts`).
 */
export function defaultKeyBarShown(
  coarsePointer: boolean,
  viewportWidth: number,
): boolean {
  return coarsePointer || viewportWidth < KEYBOARD_MIN_VIEWPORT_PX;
}

/**
 * A button on the bar. Either a key, which sends its bytes, or the Ctrl latch,
 * which sends nothing of its own and changes what the next typed character
 * sends (`ctrlSequence`, `useCtrlLatch`).
 */
export type TermBarItem =
  | { kind: "key"; key: TermKey; label: string; aria: string }
  | { kind: "ctrl"; label: string; aria: string };

// The bar's layout, left to right: the bare keys first, then Ctrl and the
// combinations common enough to be worth a button of their own, then the arrow
// cluster. `label` is what the button shows; `aria` names it for assistive
// tech, where "^C" would be read as a caret.
export const TERM_KEY_BAR: TermBarItem[] = [
  { kind: "key", key: "esc", label: "Esc", aria: "Escape" },
  { kind: "key", key: "tab", label: "Tab", aria: "Tab" },
  { kind: "key", key: "shift-tab", label: "⇧Tab", aria: "Shift Tab" },
  { kind: "ctrl", label: "Ctrl", aria: "Control for the next key" },
  { kind: "key", key: "ctrl-b", label: "^B", aria: "Control B" },
  { kind: "key", key: "ctrl-c", label: "^C", aria: "Control C" },
  { kind: "key", key: "ctrl-d", label: "^D", aria: "Control D" },
  { kind: "key", key: "ctrl-z", label: "^Z", aria: "Control Z" },
  { kind: "key", key: "ctrl-l", label: "^L", aria: "Control L" },
  { kind: "key", key: "ctrl-r", label: "^R", aria: "Control R" },
  { kind: "key", key: "left", label: "←", aria: "Left arrow" },
  { kind: "key", key: "down", label: "↓", aria: "Down arrow" },
  { kind: "key", key: "up", label: "↑", aria: "Up arrow" },
  { kind: "key", key: "right", label: "→", aria: "Right arrow" },
];
