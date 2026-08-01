import type { CSSProperties } from "react";
import { FolderPicker } from "../components/FolderPicker";
import { Header } from "../components/Header";
import { LoadingSplash } from "../components/LoadingSplash";
import { Login } from "../components/Login";
import { RepoShell } from "../components/RepoShell";
import { useAppViewModel } from "../hooks/useAppViewModel";

export function App() {
  const view = useAppViewModel();

  if (view.authed === null) return <LoadingSplash />;
  if (!view.authed) return <Login onSuccess={view.login} />;
  if (!view.reposLoaded) return <LoadingSplash />;

  return (
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
      <Header {...view.header} />
      {view.repoShell ? (
        <RepoShell {...view.repoShell} />
      ) : (
        <div className="flex items-center justify-center p-6 text-center text-ink-400">
          <span>
            No repository open. Click{" "}
            <span className="text-ink-200">+ open</span> above to add one.
          </span>
        </div>
      )}
      {view.picker && <FolderPicker {...view.picker} />}
    </div>
  );
}
