import { useCallback } from "react";
import { api, type Repo } from "../api";
import { successorOf } from "../lib/successor";
import type { Pane, Tab } from "../types";

export interface UseRepoActionsArgs {
  repos: Repo[];
  setRepos: React.Dispatch<React.SetStateAction<Repo[]>>;
  setRepo: React.Dispatch<React.SetStateAction<string | null>>;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  setTab: React.Dispatch<React.SetStateAction<Tab>>;
  setPickerOpen: React.Dispatch<React.SetStateAction<boolean>>;
  handle: (err: unknown) => void;
  orderWrites: React.MutableRefObject<number>;
}

export function useRepoActions({
  repos,
  setRepos,
  setRepo,
  setPane,
  setTab,
  setPickerOpen,
  handle,
  orderWrites,
}: UseRepoActionsArgs) {
  // Select a newly opened repository immediately instead of waiting for polling.
  const selectOpenedRepo = useCallback(
    (opened: Repo) => {
      orderWrites.current += 1;
      setRepos((prev) =>
        prev.some((r) => r.id === opened.id) ? prev : [...prev, opened],
      );
      setRepo(opened.id);
      setPane({ kind: "empty" });
      setTab("status");
      setPickerOpen(false);
    },
    [setRepos, setRepo, setPane, setTab, setPickerOpen, orderWrites],
  );

  const closeRepo = useCallback(
    async (id: string) => {
      try {
        await api.close(id);
        orderWrites.current += 1;
        // Picked here as well as on the server, which is what lasts: the poll
        // that carries the server's answer is seconds away, and until it lands
        // the person is looking at whichever project this chose. Same rule, so
        // the answer that arrives changes nothing.
        const successor = successorOf(
          repos.map((r) => r.id),
          id,
        );
        setRepos(repos.filter((r) => r.id !== id));
        setRepo((current) => (current === id ? successor : current));
      } catch (err) {
        handle(err);
      }
    },
    [repos, setRepos, setRepo, handle, orderWrites],
  );

  return { selectOpenedRepo, closeRepo };
}
