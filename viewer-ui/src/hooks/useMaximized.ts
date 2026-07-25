import { useCallback, useState } from "react";
import type { Maximized } from "../types";

// Maximize is a per-project layout choice: each repo remembers whether its
// files pane, terminal, or neither was maximized, so switching projects
// restores that project's own layout rather than carrying one over.
export function useMaximized(repo: string | null) {
  const [maximizedByRepo, setMaximizedByRepo] = useState<
    Record<string, Maximized>
  >({});
  const setMaximized = useCallback(
    (next: Maximized | ((prev: Maximized) => Maximized)) => {
      if (repo == null) return;
      setMaximizedByRepo((prev) => {
        const current = prev[repo] ?? "none";
        const value = typeof next === "function" ? next(current) : next;
        return { ...prev, [repo]: value };
      });
    },
    [repo],
  );
  const maximized: Maximized =
    (repo != null && maximizedByRepo[repo]) || "none";
  const dropMaximized = useCallback((id: string) => {
    setMaximizedByRepo(({ [id]: _closed, ...rest }) => rest);
  }, []);
  return { maximized, setMaximized, dropMaximized };
}