/**
 * OSC 52 — how a program asks the terminal it is printing to for the clipboard.
 *
 * A program on the far side of a PTY has no clipboard it can reach. `pbcopy`
 * writes to the machine hosting the session, which is not where anyone reading
 * this page is sitting, so on a viewer opened from somewhere else that copy
 * lands nowhere the reader can get at. The escape sequence is the one path that
 * ends at them: it travels with the pane's output, so it arrives wherever the
 * output is being shown.
 *
 * Nothing reports back. Claude Code prints "Copied to clipboard (63 characters)"
 * from the same branch that emits the sequence, and every other program that
 * uses it is written the same way — so a terminal that drops it leaves the
 * reader holding a confirmation and an unchanged clipboard, with no way to tell
 * which half was true. That is the reason this is answered rather than ignored.
 *
 * Parsing is kept apart from doing: what a payload asks for is decided here,
 * where it can be tested, and `lib/paneClipboard.ts` carries it out.
 */

/** The OSC identifier programs use for the clipboard. */
export const OSC_CLIPBOARD = 52;

export type Osc52Request =
  | { kind: "write"; text: string }
  /** Nothing to do — a query, a selection with no counterpart here, or a
   *  payload that did not survive decoding. */
  | { kind: "ignore" };

/**
 * Read one OSC 52 payload — everything after `ESC ] 52 ;`, which is what xterm
 * hands its handlers.
 *
 * A read query is never answered. `c;?` asks the terminal to send the clipboard
 * back *as terminal input*, which would hand whatever the reader last copied —
 * a password, a token — to whatever is running in the pane.
 *
 * Writing is allowed, and that is a trade rather than something the rest
 * entails. It does not follow from a reader already having shell access on the
 * host: the clipboard being written belongs to the device watching, which is a
 * different machine from the one the pane runs on, and it holds whatever that
 * person last copied anywhere. What is bought is the feature — a pane's copy
 * arriving at all — against a program being able to replace a clipboard that
 * gets pasted somewhere else without being looked at. Every terminal emulator
 * takes this side of that trade; reading is where they differ, and this takes
 * the strict side of it.
 */
export function parseOsc52(payload: string): Osc52Request {
  const split = payload.indexOf(";");
  if (split === -1) return { kind: "ignore" };
  const selection = payload.slice(0, split);
  const data = payload.slice(split + 1);
  if (!namesTheClipboard(selection)) return { kind: "ignore" };
  if (data === "?") return { kind: "ignore" };
  // Empty data means "clear it". Wiping what the reader copied elsewhere is not
  // something a pane's output should be able to do unasked, and no program that
  // matters here sends it.
  if (data === "") return { kind: "ignore" };
  const text = decodeUtf8Base64(data);
  if (text === null) return { kind: "ignore" };
  return { kind: "write", text };
}

/** Every selector xterm defines: `c` clipboard, `p` primary, `q` secondary,
 *  `s` select, `0`-`7` cut buffers. */
const SELECTORS = "cpqs01234567";
/** The ones a page can answer. */
const REACHABLE = "cs";

/**
 * Whether the selection this payload names is one a browser has.
 *
 * A page has exactly one clipboard, so this is a **lossy mapping and not a
 * faithful reading of the spec**: xterm makes an omitted selector mean `s0` and
 * `s` the select buffer, and both land here on the system clipboard, because
 * that is the only one there is. The X11-only destinations are dropped instead
 * of being redirected onto it — a program asking for the middle-click buffer
 * did not ask to replace what the reader last copied, and quietly widening its
 * request is the wrong way to be helpful.
 *
 * A selector carrying anything outside the defined set is not a selector, so
 * the payload is refused rather than searched for a `c` in the middle of it.
 */
function namesTheClipboard(selection: string): boolean {
  if (selection === "") return true;
  const letters = [...selection];
  if (!letters.every((letter) => SELECTORS.includes(letter))) return false;
  return letters.some((letter) => REACHABLE.includes(letter));
}

/** The text a payload carries, or null when it is not text this can trust. */
function decodeUtf8Base64(data: string): string | null {
  try {
    const binary = atob(data);
    const bytes = Uint8Array.from(binary, (c) => c.charCodeAt(0));
    // `fatal` so bytes that are not UTF-8 are refused rather than pasted as
    // replacement characters: a clipboard quietly filled with mojibake is worse
    // than one that was left alone.
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    return null;
  }
}
