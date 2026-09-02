// One place turns "action id + current leader" into the strings a control shows.
//
// Every button bound to a registry action names its key in its `title`, and the
// ones bound to a standalone chord also name it in `aria-keyshortcuts`. The
// leader is rebindable, so neither string can be typed at the call site: a
// rebinding has to move every control at once. The registry
// (`shortcutActions.ts`) says which key an action carries; this says how that
// reads, and which of the two the attribute can honestly state.

import { formatChord, type ChordSpec } from "./leaderChord";
import { actionById, type ShortcutActionId } from "./shortcutActions";

/**
 * The keys pressed in order: the leader chord then its follow-up, or a
 * standalone chord on its own. Null when nothing is bound — a leader action
 * with the leader switched off has no key, and saying otherwise would name a
 * keystroke that does nothing.
 */
export function shortcutKeys(
  id: ShortcutActionId,
  leader: ChordSpec | null,
): string[] | null {
  const action = actionById(id);
  if (action.chord) return [action.chord];
  if (!action.leader || !leader) return null;
  return [formatChord(leader), action.leader];
}

/** The key as a sentence fragment for a `title`: `Ctrl+F then t`. */
export function shortcutHintText(
  id: ShortcutActionId,
  leader: ChordSpec | null,
): string | null {
  const keys = shortcutKeys(id, leader);
  if (!keys) return null;
  return keys.join(" then ");
}

/** `title` with the key appended, or the title unchanged when there is no key. */
export function titleWithShortcut(title: string, hint: string | null): string {
  return hint === null ? title : `${title} (${hint})`;
}

// `KeyboardEvent.key` spellings, which is what `aria-keyshortcuts` is defined
// in terms of. Only Ctrl differs from the display form this module is fed.
const ARIA_KEY_NAMES: Record<string, string> = { Ctrl: "Control" };

/**
 * The `aria-keyshortcuts` value for an action, or null when the action's binding
 * is not something the attribute can state.
 *
 * A leader action deliberately has none. ARIA delimits *alternative* shortcuts
 * with a space and has no notation for a two-step sequence at all, so writing
 * `Control+F T` would be a machine-readable claim that bare `T` runs the
 * command — and bare `t` is not a binding at all. A wrong announcement costs
 * more than a missing one, the same standard that leaves the status tab and the
 * orphan recovery chip unmarked.
 *
 * Nothing is lost that is not carried elsewhere: `shortcutHintText` puts the
 * exact sequence in the control's `title`, and the shortcut sheet lists every
 * action as a labelled button with its steps in `<kbd>`.
 */
export function ariaKeyShortcuts(
  id: ShortcutActionId,
  leader: ChordSpec | null,
): string | null {
  const chord = actionById(id).chord;
  if (!chord) return null;
  // Unreachable unless the registry gains an action with both a chord and a
  // leader, which `shortcutActions.test.ts` forbids. Read back through
  // `shortcutKeys` so the display form and this one cannot drift.
  const keys = shortcutKeys(id, leader);
  return keys ? keys.map(ariaChord).join(" ") : null;
}

function ariaChord(display: string): string {
  return display
    .split("+")
    .map((part) => ARIA_KEY_NAMES[part] ?? ariaKeyName(part))
    .join("+");
}

/** A single printable key is upper case in `KeyboardEvent.key` terms once a
 *  modifier is involved, and the leader follow-ups are stored lower case. */
function ariaKeyName(part: string): string {
  return part.length === 1 ? part.toUpperCase() : part;
}
