// The classifier: one keystroke plus the leader state in, one decision out.
//
// Pure and DOM-free on purpose. Deciding whether a key is a command is the part
// that is easy to get subtly wrong — an IME composition read as a command, a
// held key firing eight times, a leader left armed forever — and none of those
// bugs need a browser to reproduce. The React layer supplies the context and
// carries out the decision; everything judgemental lives here, testable with
// plain object literals like `hardwareKeys.ts`.

import {
  SHORTCUT_ACTIONS,
  actionByLeader,
  type ShortcutAction,
} from "./shortcutActions";
import {
  chordMatches,
  literalLeaderSequence,
  parseChord,
  type ChordSpec,
} from "./leaderChord";

/** The parts of a `KeyboardEvent` this reads, so a test needs no DOM. */
export interface ShortcutKeyEvent {
  type: string;
  key: string;
  code?: string;
  keyCode?: number;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  repeat?: boolean;
  isComposing?: boolean;
}

export type ShortcutDecision =
  | { kind: "ignore" }
  | { kind: "arm" }
  /** `data` is empty when the leader chord has no terminal encoding: send nothing. */
  | { kind: "literalLeader"; data: string }
  | { kind: "cancel" }
  | { kind: "action"; action: ShortcutAction }
  | { kind: "consumed" };

export interface ShortcutContext {
  /** The configured leader, or null when the user has disabled it. */
  leader: ChordSpec | null;
  armed: boolean;
  /** From `shortcutTarget.ts::shortcutsSuppressed`. */
  suppressed: boolean;
}

// Standalone chords parsed once. A chord in the table that does not parse is a
// mistake in the table rather than in user input, so it is dropped here and
// simply never matches instead of throwing on every keystroke.
const CHORD_ACTIONS: { spec: ChordSpec; action: ShortcutAction }[] =
  SHORTCUT_ACTIONS.flatMap((action) => {
    if (!action.chord) return [];
    const spec = parseChord(action.chord);
    return spec ? [{ spec, action }] : [];
  });

// A modifier pressed on its own is the first half of a chord, not a follow-up.
const LONE_MODIFIER_KEYS = new Set(["Shift", "Control", "Alt", "Meta"]);

// What a browser reports for a key that is really an input-method composition.
// 229 is the legacy `keyCode` every engine still sends mid-composition.
const IME_KEY_CODE = 229;
const IME_KEYS = new Set(["Process", "Unidentified"]);

const IGNORE: ShortcutDecision = { kind: "ignore" };

function isImeKey(event: ShortcutKeyEvent): boolean {
  return (
    event.isComposing === true ||
    event.keyCode === IME_KEY_CODE ||
    IME_KEYS.has(event.key)
  );
}

/** Escape, or a Ctrl-only Ctrl+C: the two ways the TUI cancels a prefix. */
function isCancelKey(event: ShortcutKeyEvent): boolean {
  if (event.key === "Escape") return true;
  const ctrlOnly = event.ctrlKey && !event.shiftKey && !event.altKey && !event.metaKey;
  return ctrlOnly && event.key.toLowerCase() === "c";
}

/**
 * Classify one keystroke.
 *
 * When `ctx.suppressed` is true this always ignores — but suppression is also a
 * reason to disarm, and this function is pure and cannot. The caller feeds
 * `{ kind: "suppressed" }` to `reduceLeader` in `leaderState.ts`, which owns
 * clearing the armed state; otherwise a leader armed just before a dialog
 * opened would still be armed after it closed.
 */
export function classifyShortcutKey(
  event: ShortcutKeyEvent,
  ctx: ShortcutContext,
): ShortcutDecision {
  // Decide on keydown alone. The same keystroke also arrives as keypress and
  // keyup, and answering more than one would run the command twice.
  if (event.type !== "keydown") return IGNORE;

  // A CJK composition must never be read as a command: the keys that compose a
  // syllable are ordinary letters, and the person is writing, not commanding.
  if (isImeKey(event)) return IGNORE;

  // One physical press runs a command once. Holding a key down repeats the
  // keydown, and repeating "close the pane" is not what holding it means.
  if (event.repeat === true) return IGNORE;

  // Somebody else owns the keyboard right now (a text field, a dialog, an
  // active composition). See the doc comment on disarming.
  if (ctx.suppressed) return IGNORE;

  if (ctx.armed) {
    if (isCancelKey(event)) return { kind: "cancel" };

    // The leader twice over sends one literal leader chord to the pane, so a
    // program that binds the same chord is still reachable.
    if (ctx.leader && chordMatches(ctx.leader, event)) {
      return {
        kind: "literalLeader",
        data: literalLeaderSequence(ctx.leader) ?? "",
      };
    }

    // Modifiers on the follow-up are ignored, mirroring the TUI's
    // `prefix_action`: the leader chord's Ctrl is often still held when the
    // next key lands, and that must not change which command it names.
    if (event.key.length === 1) {
      const action = actionByLeader(event.key);
      if (action) return { kind: "action", action };
    }

    // A modifier alone leaves the leader armed for the follow-up still to come.
    if (LONE_MODIFIER_KEYS.has(event.key)) return IGNORE;

    // An unmapped follow-up spends the leader and goes no further, exactly as
    // `docs/keybindings.md` states: "An unmapped follow-up is consumed." It
    // must not reach the PTY, or `<prefix> j` would type a `j` into the shell.
    return { kind: "consumed" };
  }

  if (ctx.leader && chordMatches(ctx.leader, event)) return { kind: "arm" };

  for (const { spec, action } of CHORD_ACTIONS) {
    if (chordMatches(spec, event)) return { kind: "action", action };
  }

  // Anything not explicitly claimed above belongs to the page and the pane.
  return IGNORE;
}
