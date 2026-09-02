import type { CSSProperties } from "react";
import { Brand } from "../components/Brand";
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
      {/* The left strip is a column beside the grid, headed by the title so
          the tabs hang under the name rather than in a panel of their own; the
          title's row is the header's height, so the two line up. */}
      <div className="nc-fade flex h-full">
        {view.tabStrip.side === "left" && (
          <aside className="hidden w-48 shrink-0 flex-col bg-ink-900 md:flex">
            {/* The header's exact height (see `Header`), so this border and the
                header's are one line at one pixel row. */}
            <div className="flex h-[42px] items-center gap-2 border-b border-ink-700 px-[12.8px]">
              <Brand />
            </div>
            <ProjectStrip side="left" {...view.header} />
          </aside>
        )}
        <div
          className={`grid h-full min-w-0 flex-1 ${view.rows}`}
          style={
            {
              // `fr` pairs divide only the space left between fixed chrome rows.
              "--nc-upper": `${view.upperPct}fr`,
              "--nc-lower": `${100 - view.upperPct}fr`,
            } as CSSProperties
          }
        >
          <Header {...view.header} onShowShortcuts={view.shortcutHelp.show} />
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
                {view.tabStrip.side === "left" ? "beside" : "above"} to add one.
              </span>
            </div>
          )}
          {view.picker && <FolderPicker {...view.picker} />}
          {view.shortcutHelp.open && (
            <ShortcutHelp onClose={view.shortcutHelp.hide} leader={view.leader} />
          )}
        </div>
      </div>
    </ShortcutLeaderProvider>
  );
}
