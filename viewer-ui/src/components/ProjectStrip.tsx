import { PlusIcon, XIcon } from "./icons/actions";
import { useLeaderChord, useShortcutHint } from "../hooks/shortcutLeader";
import { ariaKeyShortcuts, shortcutHintText } from "../lib/shortcutHint";
import { tabLabel } from "../lib/tabLabel";
import type { TabStripSide } from "../lib/tabStripSide";
import type { Repo } from "../api";

export interface ProjectStripProps {
  /** Across the header, or down the page's left edge — the same tabs, laid
   *  the other way. Below `md` neither is drawn; the header's project menu
   *  stands in. */
  side: TabStripSide;
  repos: Repo[];
  repo: string | null;
  onSelectRepo: (id: string) => void;
  onCloseRepo: (id: string) => void;
  onOpenPicker: () => void;
  draggingRepo: string | null;
  dragOverRepo: string | null;
  onRepoDragStart: (event: React.PointerEvent, id: string) => void;
  onRepoDragMove: (event: React.PointerEvent) => void;
  onRepoDragEnd: () => void;
}

/**
 * The project tabs, and the control that opens another.
 *
 * One component for both placements so a tab is the same tab wherever the
 * strip runs: the label rule, the drag handles, the close control and the
 * keys it advertises. Only what depends on the axis differs — how the strip
 * scrolls, where a tab's accent bar sits, and which edge the tabs abut.
 */
export function ProjectStrip({
  side,
  repos,
  repo,
  onSelectRepo,
  onCloseRepo,
  onOpenPicker,
  draggingRepo,
  dragOverRepo,
  onRepoDragStart,
  onRepoDragMove,
  onRepoDragEnd,
}: ProjectStripProps) {
  const shortcut = useShortcutHint();
  const leader = useLeaderChord();
  // The two project chords act on the strip, not on any one tab: they move the
  // selection relative to the project in front, so no tab can claim either as
  // the key that activates it. Named here, where the strip is, as the pair of
  // alternatives ARIA's space-separated list is for — and only while there is
  // somewhere to go, because with one project open the chords are not
  // registered and naming them would announce a key that does nothing.
  const cycleActions =
    repos.length > 1 ? (["project.previous", "project.next"] as const) : [];
  const named = <T,>(value: T | null): value is T => value !== null;
  const cycleKeys = cycleActions
    .map((id) => ariaKeyShortcuts(id, leader))
    .filter(named);
  const cycleHint = cycleActions
    .map((id) => shortcutHintText(id, leader))
    .filter(named);
  const left = side === "left";
  return (
    <>
      <nav
        aria-keyshortcuts={cycleKeys.length > 0 ? cycleKeys.join(" ") : undefined}
        title={
          cycleHint.length > 0
            ? `Previous or next project (${cycleHint.join(", ")})`
            : undefined
        }
        className={
          left
            ? "hidden min-h-0 flex-1 flex-col overflow-y-auto md:flex"
            : "hidden items-stretch self-stretch overflow-x-auto pl-1 md:flex"
        }
      >
        {repos.map((r) => (
          <div
            key={r.id}
            data-repo-id={r.id}
            onPointerDown={(event) => onRepoDragStart(event, r.id)}
            onPointerMove={onRepoDragMove}
            onPointerUp={onRepoDragEnd}
            onPointerCancel={onRepoDragEnd}
            onLostPointerCapture={onRepoDragEnd}
            className={`flex items-center whitespace-nowrap ${
              left ? "border-b border-ink-700" : "border-r border-ink-700"
            } ${repos.length > 1 ? "touch-none" : ""} ${
              draggingRepo === r.id ? "opacity-60" : ""
            } ${
              dragOverRepo === r.id ? "bg-ink-800 ring-1 ring-inset ring-accent" : ""
            } ${
              r.id === repo
                ? `bg-ink-950 text-ink-50 ${
                    left
                      ? "shadow-[inset_2px_0_0_0_var(--color-accent)]"
                      : "shadow-[inset_0_2px_0_0_var(--color-accent)]"
                  }`
                : "text-ink-400 hover:bg-ink-850 hover:text-ink-200"
            }`}
            title={r.display_path}
          >
            {/* Shortened here, not by the server: `name` is what the project
                menu and the labels below read out, and those want it whole. */}
            <button
              onClick={() => {
                onSelectRepo(r.id);
              }}
              aria-label={r.name}
              className={`self-stretch pl-3 pr-1 ${left ? "flex-1 py-2 text-left" : ""}`}
            >
              {tabLabel(r.name)}
            </button>
            <button
              onClick={(e) => {
                e.stopPropagation();
                onCloseRepo(r.id);
              }}
              data-tab-close
              // Only on the project in front: `project.close` closes the current
              // project, so on any other tab the key would name a different one.
              {...(r.id === repo
                ? shortcut("project.close", "Close project")
                : { title: "Close project" })}
              aria-label={`close ${r.name}`}
              className="mr-1 flex h-5 w-5 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-removed"
            >
              <XIcon className="h-3.5 w-3.5" />
            </button>
          </div>
        ))}
      </nav>
      <button
        onClick={onOpenPicker}
        {...shortcut("project.openDialog", "Open a project")}
        className={`hidden shrink-0 items-center gap-1 rounded-sm px-2 py-0.5 text-ink-400 hover:text-ink-200 md:inline-flex ${
          left ? "mx-1 my-2 justify-center" : ""
        }`}
      >
        <PlusIcon className="h-3.5 w-3.5" />
        open
      </button>
    </>
  );
}
