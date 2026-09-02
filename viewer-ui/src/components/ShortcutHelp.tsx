import { useEffect, useRef } from "react";
import { XIcon } from "./icons/actions";
import { LeaderSettings } from "./shortcuts/LeaderSettings";
import { ShortcutRow } from "./shortcuts/ShortcutRow";
import { useShortcutIntents, useShortcutAvailability } from "../hooks/shortcutIntents";
import type { ShortcutSettings } from "../hooks/useShortcutSettings";
import { ariaKeyShortcuts, shortcutKeys } from "../lib/shortcutHint";
import {
  SHORTCUT_ACTIONS,
  UNSUPPORTED_TUI_ACTIONS,
  type ShortcutAction,
  type ShortcutGroup,
} from "../lib/shortcutActions";

// What the keyboard can do, said out loud.
//
// Every row here comes from `SHORTCUT_ACTIONS`: the sheet holds no key table of
// its own, so a command added to the registry appears here without anybody
// remembering to list it, and a rebinding moves what is printed. The rows are
// buttons because a shortcut with no other way to reach it is not a feature on
// a touch screen — `focus.list` and `focus.content` have no button anywhere else
// in the viewer, and this is theirs.

/** Group headings. A `Record` of the union, so adding a group to the registry
 *  fails the build here rather than rendering an untitled section. */
const GROUP_TITLES: Record<ShortcutGroup, string> = {
  terminal: "Terminal",
  project: "Projects",
  view: "Layout and views",
  focus: "Focus",
  session: "Session",
  help: "Help",
};

/** Sections in the registry's own order, so the sheet reads in the order the
 *  table is written and never needs a second list of group names. */
const GROUPS: readonly { group: ShortcutGroup; actions: ShortcutAction[] }[] =
  (() => {
    const sections: { group: ShortcutGroup; actions: ShortcutAction[] }[] = [];
    for (const action of SHORTCUT_ACTIONS) {
      const last = sections.find((s) => s.group === action.group);
      if (last) last.actions.push(action);
      else sections.push({ group: action.group, actions: [action] });
    }
    return sections;
  })();

const TITLE_ID = "nc-shortcut-help-title";

export function ShortcutHelp({
  onClose,
  leader,
}: {
  onClose: () => void;
  /** The whole leader preference: the sheet prints the chord and is also where
   *  it is rebound. */
  leader: ShortcutSettings;
}) {
  const intents = useShortcutIntents();
  const isAvailable = useShortcutAvailability();
  const sheetRef = useRef<HTMLDivElement>(null);
  // Whether a row ran a command on the way out. `focus.list` and
  // `focus.content` move the keyboard somewhere on purpose, and handing it back
  // to the opener afterwards would undo the only thing they do.
  const ranAction = useRef(false);

  useEffect(() => {
    const opener = document.activeElement;
    sheetRef.current?.focus();
    return () => {
      if (ranAction.current) return;
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
    };
  }, []);

  // On the document, matching `ProjectMenu`: the sheet is modal, so Escape
  // closes it wherever inside it the keyboard happens to be.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  const run = (action: ShortcutAction) => {
    ranAction.current = true;
    intents?.runAction(action.id);
    onClose();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4"
      onClick={onClose}
    >
      <div
        ref={sheetRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={TITLE_ID}
        // Focusable but not a Tab stop: the sheet takes the keyboard on open so
        // Escape and the arrow keys land inside it, and Tab then continues into
        // the rows.
        tabIndex={-1}
        className="flex max-h-[85vh] w-[40rem] max-w-full flex-col rounded-md border border-ink-700 bg-ink-900 focus:outline-none"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex shrink-0 items-center gap-2 border-b border-ink-700 px-3 py-2">
          <span id={TITLE_ID} className="font-medium text-ink-50">
            Keyboard shortcuts
          </span>
          <button
            type="button"
            onClick={onClose}
            aria-label="close"
            className="ml-auto flex h-6 w-6 items-center justify-center rounded-sm text-ink-400 hover:text-ink-200"
          >
            <XIcon />
          </button>
        </div>
        <LeaderSettings settings={leader} />
        <div className="min-h-0 flex-1 overflow-y-auto py-1">
          {GROUPS.map(({ group, actions }) => (
            <section key={group}>
              <h3 className="px-3 py-1 text-[0.7rem] uppercase tracking-[0.14em] text-ink-400">
                {GROUP_TITLES[group]}
              </h3>
              <ul>
                {actions.map((action) => (
                  <ShortcutRow
                    key={action.id}
                    action={action}
                    keys={shortcutKeys(action.id, leader.leader)}
                    ariaKeys={ariaKeyShortcuts(action.id, leader.leader)}
                    available={isAvailable(action.id)}
                    onRun={() => run(action)}
                  />
                ))}
              </ul>
            </section>
          ))}
          {/* Listed rather than left out: the gaps against the TUI are a
              deliberate part of this binding, and a sheet that simply omitted
              them would read as unfinished. */}
          <section>
            <h3 className="px-3 py-1 text-[0.7rem] uppercase tracking-[0.14em] text-ink-400">
              Not bound in the browser
            </h3>
            <ul>
              {UNSUPPORTED_TUI_ACTIONS.map((entry) => (
                <li key={entry.leader} className="px-3 py-1.5">
                  <span className="flex items-baseline gap-2">
                    <span className="min-w-0 flex-1 text-ink-200">
                      {entry.label}
                    </span>
                    <kbd className="shrink-0 rounded-sm border border-ink-700 bg-ink-850 px-1.5 py-0.5 font-mono text-ink-400">
                      {entry.leader}
                    </kbd>
                  </span>
                  <span className="text-ink-400">{entry.reason}</span>
                </li>
              ))}
            </ul>
          </section>
        </div>
      </div>
    </div>
  );
}
