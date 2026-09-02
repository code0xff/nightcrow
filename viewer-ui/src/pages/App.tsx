import type { CSSProperties } from "react";
import { FolderPicker } from "../components/FolderPicker";
import { Header } from "../components/Header";
import { LoadingSplash } from "../components/LoadingSplash";
import { Login } from "../components/Login";
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
      <div
        className={`nc-fade grid h-full ${view.rows}`}
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
              <span className="text-ink-200">+ open</span> above to add one.
            </span>
          </div>
        )}
        {view.picker && <FolderPicker {...view.picker} />}
        {view.shortcutHelp.open && (
          <ShortcutHelp onClose={view.shortcutHelp.hide} leader={view.leader} />
        )}
      </div>
    </ShortcutLeaderProvider>
  );
}
