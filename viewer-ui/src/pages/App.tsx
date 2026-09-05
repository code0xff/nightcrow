import type { CSSProperties } from "react";
import { FolderPicker } from "../components/FolderPicker";
import { Header } from "../components/Header";
import { LoadingSplash } from "../components/LoadingSplash";
import { Login } from "../components/Login";
import { ProjectStrip } from "../components/ProjectStrip";
import { RepoShell } from "../components/RepoShell";
import { ShortcutHelp } from "../components/ShortcutHelp";
import { ShortcutHintBar } from "../components/shortcuts/ShortcutHintBar";
import { ShortcutLeaderProvider } from "../hooks/shortcutLeader";
import { useAppViewModel } from "../hooks/useAppViewModel";

export function App() {
  const view = useAppViewModel();

  if (view.authed === null) return <LoadingSplash />;
  if (!view.authed) return <Login onSuccess={view.login} />;
  if (!view.reposLoaded) return <LoadingSplash />;

  return (
    // The leader, for every control that names its own shortcut. Provided here
    // rather than in `main.tsx` because this is where the settings object comes
    // out of the view model, and one source is what keeps a rebinding from
    // moving some controls and leaving others behind.
    <ShortcutLeaderProvider leader={view.leader.leader}>
      {/* The header spans the page, so its one border is the one line under
          the title and the controls alike. Below it the left strip, when
          chosen, is a column beside the grid: the tabs hang under the title in
          the header's left corner rather than in a panel with a title of its
          own — which would mean a second border, never quite the first. */}
      <div className="nc-fade flex h-full flex-col">
        <Header {...view.header} onShowShortcuts={view.shortcutHelp.show} />
        <div className="flex min-h-0 flex-1">
          {view.tabStrip.side === "left" && (
            <aside className="hidden w-48 shrink-0 flex-col border-r border-ink-700 bg-ink-950 md:flex">
              <ProjectStrip side="left" {...view.header} />
            </aside>
          )}
          {/* The one column is pinned to `minmax(0,1fr)` rather than left
              `auto`: an auto track cannot be narrower than the widest row's
              min-content, and `truncate` sets `white-space: nowrap`, whose
              min-content *is* its max-content. The footer's path and branch
              would then set the shell's width, and every row under them —
              panels, mobile nav, footer — would stretch to it while the
              viewport scrolled sideways underneath. */}
          <div
            className={`grid min-h-0 min-w-0 flex-1 grid-cols-[minmax(0,1fr)] ${view.rows}`}
            style={
              {
                // `fr` pairs divide only the space left between fixed chrome rows.
                "--nc-upper": `${view.upperPct}fr`,
                "--nc-lower": `${100 - view.upperPct}fr`,
              } as CSSProperties
            }
          >
            {view.repoShell ? (
              <>
                <RepoShell {...view.repoShell} />
                {/* Last, under the footer, where the TUI keeps it. */}
                <ShortcutHintBar {...view.hint} />
              </>
            ) : (
              <div className="flex items-center justify-center p-6 text-center text-ink-400">
                <span>
                  No repository open. Click{" "}
                  <span className="text-ink-200">+ open</span>{" "}
                  {view.tabStrip.side === "left" ? "beside" : "above"} to add
                  one.
                </span>
              </div>
            )}
            {view.picker && <FolderPicker {...view.picker} />}
            {view.shortcutHelp.open && (
              <ShortcutHelp
                onClose={view.shortcutHelp.hide}
                leader={view.leader}
              />
            )}
          </div>
        </div>
      </div>
    </ShortcutLeaderProvider>
  );
}
