import { SearchIcon } from "./icons/actions";
import { useShortcutHint } from "../hooks/shortcutLeader";
import type { ShortcutActionId } from "../lib/shortcutActions";
import type { Tab } from "../types";

// Lifted out of `Sidebar` unchanged, because the keys these three tabs answer to
// have to be named on them and `Sidebar` had no room left: the row is a self
// contained control strip whose only inputs are which list is showing and
// whether the filter is open.

const TABS: readonly Tab[] = ["status", "log", "tree"];

const TAB_TITLES: Record<Tab, string> = {
  status: "Show the working-tree status",
  log: "Show the commit log",
  tree: "Show the file tree",
};

/** The action a tab is, or null for one no single action names. `status` is the
 *  *off* state of both toggles — from the log it is `view.toggleLog` and from the
 *  tree it is `view.toggleTree` — so no one key belongs on it. */
function tabAction(tab: Tab): ShortcutActionId | null {
  if (tab === "log") return "view.toggleLog";
  if (tab === "tree") return "view.toggleTree";
  return null;
}

export function SidebarTabs({
  tab,
  onChoose,
  filterOpen,
  onToggleFilter,
}: {
  tab: Tab;
  onChoose: (next: Tab) => void;
  filterOpen: boolean;
  onToggleFilter: () => void;
}) {
  const shortcut = useShortcutHint();
  return (
    <div className="flex shrink-0 items-stretch border-b border-ink-700 px-2">
      {TABS.map((t) => {
        const action = tabAction(t);
        return (
          <button
            key={t}
            onClick={() => onChoose(t)}
            aria-current={t === tab ? "page" : undefined}
            // Not on the tab already showing: there the toggle goes back to the
            // status list, which is not what pressing this tab does.
            {...(action && t !== tab
              ? shortcut(action, TAB_TITLES[t])
              : { title: TAB_TITLES[t] })}
            className={`-mb-px border-b-2 px-2 py-1 ${
              t === tab
                ? "border-accent text-ink-50"
                : "border-transparent text-ink-400 hover:text-ink-200"
            }`}
          >
            {t}
          </button>
        );
      })}
      <button
        onClick={onToggleFilter}
        aria-pressed={filterOpen}
        title={filterOpen ? "Hide the filter" : "Filter the list"}
        aria-label={filterOpen ? "Hide the filter" : "Filter the list"}
        className={`my-1 ml-auto flex shrink-0 items-center rounded-sm px-1.5 hover:text-accent ${
          filterOpen ? "text-ink-50" : "text-ink-400"
        }`}
      >
        <SearchIcon />
      </button>
    </div>
  );
}
