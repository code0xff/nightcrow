import { useCallback } from "react";
import { api, type Repo } from "../api";
import type { Pane, Tab } from "../types";

export interface UseRepoActionsArgs {
  repos: Repo[];
  setRepos: React.Dispatch<React.SetStateAction<Repo[]>>;
  setRepo: React.Dispatch<React.SetStateAction<string | null>>;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  setTab: React.Dispatch<React.SetStateAction<Tab>>;
  setPickerOpen: React.Dispatch<React.SetStateAction<boolean>>;
  dropMaximized: (id: string) => void;
  handle: (err: unknown) => void;
}

export function useRepoActions({
  repos,
  setRepos,
  setRepo,
  setPane,
  setTab,
  setPickerOpen,
  dropMaximized,
  handle,
}: UseRepoActionsArgs) {
  // Select a newly opened repository immediately instead of waiting for polling.
  const selectOpenedRepo = useCallback(
    (opened: Repo) => {
      setRepos((prev) =>
        prev.some((r) => r.id === opened.id) ? prev : [...prev, opened],
      );
      setRepo(opened.id);
      setPane({ kind: "empty" });
      setTab("status");
      setPickerOpen(false);
    },
    [setRepos, setRepo, setPane, setTab, setPickerOpen],
  );

  const closeRepo = useCallback(
    async (id: string) => {
      try {
        await api.close(id);
        const remaining = repos.filter((r) => r.id !== id);
        setRepos(remaining);
        setRepo((current) =>
          current === id ? (remaining[0]?.id ?? null) : current,
        );
        dropMaximized(id);
      } catch (err) {
        handle(err);
      }
    },
    [repos, setRepos, setRepo, dropMaximized, handle],
  );

  return { selectOpenedRepo, closeRepo };
}
