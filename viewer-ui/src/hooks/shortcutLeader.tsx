import { createContext, useCallback, useContext, type ReactNode } from "react";
import type { ChordSpec } from "../lib/leaderChord";
import {
  ariaKeyShortcuts,
  shortcutHintText,
  titleWithShortcut,
} from "../lib/shortcutHint";
import type { ShortcutActionId } from "../lib/shortcutActions";

// The current leader, published to every control that has to name its key.
//
// A context rather than props: the controls that need it are the accent swatch
// in the header, the toolbar inside the terminal panel and the button in a
// terminal cell's title row, and threading a chord through `RepoShell` and
// `Terminal` to reach them would put the keyboard in the signature of every
// component in between. A second `useShortcutSettings()` at each call site is
// not an option either — it reads `localStorage` once per mount, so a rebinding
// would move some controls and leave others behind.
//
// Deliberately *not* part of the intent bus: the bus answers "who can run this
// right now" and changes identity whenever a pane opens; the leader changes only
// when somebody rebinds it, and a control that shows a key has no reason to
// re-render for the other thing.

const LeaderContext = createContext<ChordSpec | null>(null);

export function ShortcutLeaderProvider({
  leader,
  children,
}: {
  leader: ChordSpec | null;
  children: ReactNode;
}) {
  return (
    <LeaderContext.Provider value={leader}>{children}</LeaderContext.Provider>
  );
}

/**
 * The configured leader chord, or null when it is switched off.
 *
 * Null is also what a component rendered outside the provider reads, and that
 * is the wanted answer: no leader is known, so no control claims a leader key.
 * The alternative — a throw — would make an isolated component test of any
 * toolbar depend on this context existing.
 */
export function useLeaderChord(): ChordSpec | null {
  return useContext(LeaderContext);
}

/** What a control spreads to name its shortcut: the human title with the key
 *  appended, and the ARIA value. The attribute is absent for a leader sequence,
 *  which ARIA cannot state — see `ariaKeyShortcuts`. */
export interface ShortcutHintProps {
  title: string;
  "aria-keyshortcuts"?: string;
}

/**
 * `title` and `aria-keyshortcuts` for a control that runs a registry action.
 *
 * Spread it in place of the `title` the control already had. Returns a function
 * rather than one action's props so a toolbar with four bound buttons calls one
 * hook instead of four.
 *
 * The `title` always carries the key. `aria-keyshortcuts` appears only for an
 * action bound to a standalone chord: a leader sequence has no ARIA spelling
 * that does not claim something false.
 */
export function useShortcutHint(): (
  id: ShortcutActionId,
  title: string,
) => ShortcutHintProps {
  const leader = useLeaderChord();
  return useCallback(
    (id: ShortcutActionId, title: string) => {
      const keys = ariaKeyShortcuts(id, leader);
      const props: ShortcutHintProps = {
        title: titleWithShortcut(title, shortcutHintText(id, leader)),
      };
      if (keys !== null) props["aria-keyshortcuts"] = keys;
      return props;
    },
    [leader],
  );
}
