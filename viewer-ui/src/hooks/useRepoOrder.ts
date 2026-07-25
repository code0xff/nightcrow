import { useCallback } from "react";
import { api, type Repo } from "../api";
import { reconcileOrder } from "../lib/paneOrder";
import { useRepoDrag } from "./useRepoDrag";

interface UseRepoOrderArgs {
  repos: Repo[];
  setRepos: React.Dispatch<React.SetStateAction<Repo[]>>;
  handle: (error: unknown) => void;
  writesRef: React.MutableRefObject<number>;
  draggingRef: React.MutableRefObject<boolean>;
  inFlightRef: React.MutableRefObject<boolean>;
  pendingRef: React.MutableRefObject<string[] | null>;
}

export function useRepoOrder({
  repos,
  setRepos,
  handle,
  writesRef,
  draggingRef,
  inFlightRef,
  pendingRef,
}: UseRepoOrderArgs) {

  const flush = useCallback(() => {
    if (inFlightRef.current || pendingRef.current === null) return;
    const order = pendingRef.current;
    pendingRef.current = null;
    inFlightRef.current = true;
    const generation = writesRef.current;
    api
      .reorderRepos(order)
      .then((serverRepos) => {
        if (writesRef.current === generation) setRepos(serverRepos);
      })
      .catch(handle)
      .finally(() => {
        inFlightRef.current = false;
        flush();
      });
  }, [handle, setRepos]);

  const commit = useCallback(
    (order: string[]) => {
      writesRef.current += 1;
      setRepos((current) => {
        const ids = reconcileOrder(
          current.map((repo) => repo.id),
          order,
        );
        const byId = new Map(current.map((repo) => [repo.id, repo]));
        return ids.map((id) => byId.get(id)!).filter(Boolean);
      });
      pendingRef.current = order;
      flush();
    },
    [flush, setRepos],
  );

  const drag = useRepoDrag({
    ids: repos.map((repo) => repo.id),
    onReorder: commit,
    draggingRef,
  });

  return {
    ...drag,
    writesRef,
    draggingRef,
    inFlightRef,
    pendingRef,
  };
}
