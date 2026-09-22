// Keys a physical keyboard sends that xterm.js encodes as something else.
//
// xterm answers Enter with CR whatever modifier is held, so Ctrl+Enter — the
// chord a TUI reads as "insert a newline, don't submit" — arrives at the pane
// as a submit. A browser has no trouble telling the two apart, which is what
// makes this worth overriding: the information is there and only the encoding
// throws it away.
//
// The TUI encodes the same key in `src/input/encode.rs`. The two faces of a
// session share panes, so a key that meant one thing in the terminal and
// another in the page would be a difference the person typing cannot see.

/** The parts of a `KeyboardEvent` this reads, so a test needs no DOM. */
export interface TypedKey {
  type: string;
  key: string;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** Ctrl+J under another name, and the byte a TUI reads as a newline. */
const LF = "\n";
const ESC = "\x1b";

/**
 * The bytes to send in xterm's place, or null to leave the key to xterm.
 *
 * Only a keydown answers: the same handler sees keypress for the same
 * keystroke, and answering both would send the newline twice. A held Meta is
 * left alone — that is the window manager's chord, not the pane's.
 */
export function overriddenKeySequence(event: TypedKey): string | null {
  if (event.type !== "keydown") return null;
  if (event.key !== "Enter" || !event.ctrlKey || event.metaKey) return null;
  // Alt takes the Meta prefix, exactly as it does for Alt+Enter.
  return event.altKey ? ESC + LF : LF;
}

/**
 * Whether this key should be left to the browser rather than encoded by xterm.
 *
 * To a terminal Ctrl+V is a control character (SYN, readline's quoted insert),
 * so xterm encodes it and calls `preventDefault` and no `paste` event is ever
 * fired. In this panel that costs more than the byte is worth: pasting an
 * image depends on the `paste` event entirely (`lib/pasteImage.ts`), and the
 * clipboard it reads belongs to the device showing the page, which nothing on
 * the far side of the PTY can reach. Quoted insert is what this gives up;
 * Shift+Insert still pastes for anyone who had been using it.
 *
 * Cmd+V is not listed: xterm does not encode a held Meta, so on a Mac the
 * browser already gets the key.
 */
export function browserHandlesKey(event: TypedKey): boolean {
  if (event.type !== "keydown") return false;
  return (
    event.ctrlKey &&
    !event.altKey &&
    !event.metaKey &&
    (event.key === "v" || event.key === "V")
  );
}
