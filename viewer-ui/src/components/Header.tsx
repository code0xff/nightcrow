import { Brand } from "./Brand";
import { ProjectMenu } from "./ProjectMenu";
import { ProjectStrip } from "./ProjectStrip";
import { LogOutIcon, RefreshIcon } from "./icons/actions";
import { KeyboardIcon, TabStripIcon } from "./icons/layout";
import { useShortcutHint } from "../hooks/shortcutLeader";
import type { TabStrip } from "../hooks/ui/tabStripSide";
import { otherSide } from "../lib/tabStripSide";
import type { Repo } from "../api";

export interface HeaderProps {
  repos: Repo[];
  repo: string | null;
  onSelectRepo: (id: string) => void;
  onCloseRepo: (id: string) => void;
  onOpenPicker: () => void;
  /** A clone is running on the server. Shown here rather than in the folder
   *  picker because the job outlives that dialog. */
  cloning: boolean;
  accent: { name: string };
  next: { name: string };
  cycle: () => void;
  draggingRepo: string | null;
  dragOverRepo: string | null;
  onRepoDragStart: (event: React.PointerEvent, id: string) => void;
  onRepoDragMove: (event: React.PointerEvent) => void;
  onRepoDragEnd: () => void;
  /** Owned by the page, not by this component: the keyboard reloads the config
   *  too, and two instances of `useReloadConfig` would each hold their own
   *  in-flight guard. */
  onReloadConfig: () => void;
  reloading: boolean;
  /** Open the shortcut sheet. Held by the page beside the keyboard that opens
   *  the same sheet, so both go through one piece of state. */
  onShowShortcuts: () => void;
  /** Where the project strip is. The header draws it only across the top; on
   *  the left the page draws it beside the whole grid, and the header keeps the
   *  control that moves it. */
  tabStrip: TabStrip;
}

export function Header({
  repos,
  repo,
  onSelectRepo,
  onCloseRepo,
  onOpenPicker,
  cloning,
  accent,
  next,
  cycle,
  draggingRepo,
  dragOverRepo,
  onRepoDragStart,
  onRepoDragMove,
  onRepoDragEnd,
  onReloadConfig,
  reloading,
  onShowShortcuts,
  tabStrip,
}: HeaderProps) {
  const shortcut = useShortcutHint();
  return (
    <header className="flex items-center gap-2 border-b border-ink-700 bg-ink-900 px-[12.8px] py-[8.8px]">
      {/* With the tabs on the left the title heads their column instead, so
          the header shows it only where that column is not drawn. */}
      <div
        className={`flex items-center gap-2 ${
          tabStrip.side === "left" ? "md:hidden" : ""
        }`}
      >
        <Brand />
      </div>
      <ProjectMenu
        className="md:hidden"
        repos={repos}
        currentId={repo}
        onSelect={onSelectRepo}
        onCloseProject={onCloseRepo}
        onOpenPicker={onOpenPicker}
      />
      {tabStrip.side === "top" && (
        <ProjectStrip
          side="top"
          repos={repos}
          repo={repo}
          onSelectRepo={onSelectRepo}
          onCloseRepo={onCloseRepo}
          onOpenPicker={onOpenPicker}
          draggingRepo={draggingRepo}
          dragOverRepo={dragOverRepo}
          onRepoDragStart={onRepoDragStart}
          onRepoDragMove={onRepoDragMove}
          onRepoDragEnd={onRepoDragEnd}
        />
      )}
      {cloning && (
        <span
          role="status"
          title="A clone is running on the server"
          className="flex shrink-0 items-center gap-1.5 px-2 py-0.5 text-ink-400"
        >
          <span
            aria-hidden="true"
            className="h-1.5 w-1.5 animate-pulse rounded-full bg-accent"
          />
          Cloning…
        </span>
      )}
      <button
        onClick={cycle}
        {...shortcut(
          "session.cycleAccent",
          `Accent: ${accent.name} (click for ${next.name})`,
        )}
        aria-label={`accent colour: ${accent.name}, click for ${next.name}`}
        className="ml-auto flex h-6 w-6 shrink-0 items-center justify-center rounded-sm"
      >
        <span
          aria-hidden="true"
          className="h-3 w-3 rounded-full bg-accent ring-1 ring-ink-600"
        />
      </button>
      {/* Wide screens only, like the strip it moves: below `md` there is no
          strip, and a control for where it goes would be a control for nothing. */}
      <button
        onClick={tabStrip.toggle}
        title={`Project tabs ${tabStrip.side === "top" ? "across the top" : "down the left"} (click to move them ${otherSide(tabStrip.side)})`}
        aria-label={`project tabs: ${tabStrip.side}, click for ${otherSide(tabStrip.side)}`}
        aria-pressed={tabStrip.side === "left"}
        className="ml-1 hidden h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-ink-200 md:flex"
      >
        <TabStripIcon side={tabStrip.side} />
      </button>
      {/* The title says "config" because the shape does not: a circular arrow
          reads as a browser refresh, and this reloads the server's config.toml
          while leaving the page exactly as it is. */}
      <button
        onClick={onReloadConfig}
        disabled={reloading}
        {...shortcut(
          "session.reloadConfig",
          "Reload config.toml on the server (does not reload this page)",
        )}
        aria-label="reload the server config"
        className="ml-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-ink-200 disabled:cursor-progress disabled:text-ink-500 disabled:hover:bg-transparent"
      >
        <RefreshIcon
          className={`h-3.5 w-3.5 ${reloading ? "animate-spin" : ""}`}
        />
      </button>
      <button
        onClick={onShowShortcuts}
        {...shortcut("help.shortcuts", "Keyboard shortcuts")}
        aria-label="keyboard shortcuts"
        className="ml-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-ink-200"
      >
        <KeyboardIcon className="h-3.5 w-3.5" />
      </button>
      <a
        href="/logout"
        title="Sign out"
        aria-label="sign out"
        className="ml-1 flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-ink-400 hover:bg-ink-700 hover:text-ink-200"
      >
        <LogOutIcon className="h-3.5 w-3.5" />
      </a>
    </header>
  );
}
