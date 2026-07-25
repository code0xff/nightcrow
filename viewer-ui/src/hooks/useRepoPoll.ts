import { useEffect, useState } from "react";
import {
  api,
  isNetworkError,
  isUnauthorized,
  type HotConfig,
  type Repo,
} from "../api";
import { nextClockOffset } from "../lib/hot";
import { reconcileOrder } from "../lib/paneOrder";

const REPO_POLL_MS = 3000;

export interface UseRepoPollArgs {
  authed: boolean | null;
  setAuthed: React.Dispatch<React.SetStateAction<boolean | null>>;
  handle: (err: unknown) => void;
  adoptAccent: (accent: number) => void;
  adoptSidebarWidth: (px: number) => void;
  draggingRef: React.MutableRefObject<boolean>;
  accentWrites: React.MutableRefObject<number>;
  sidebarWrites: React.MutableRefObject<number>;
  resumeTick: number;
  orderWrites: React.MutableRefObject<number>;
  repoDraggingRef: React.MutableRefObject<boolean>;
  reorderInFlightRef: React.MutableRefObject<boolean>;
  pendingReorderRef: React.MutableRefObject<string[] | null>;
}

export interface UseRepoPollResult {
  repos: Repo[];
  setRepos: React.Dispatch<React.SetStateAction<Repo[]>>;
  repo: string | null;
  setRepo: React.Dispatch<React.SetStateAction<string | null>>;
  hot: HotConfig | null;
  clockSkewMs: number | null;
  reposLoaded: boolean;
}

export function useRepoPoll({
  authed,
  setAuthed,
  handle,
  adoptAccent,
  adoptSidebarWidth,
  draggingRef,
  accentWrites,
  sidebarWrites,
  resumeTick,
  orderWrites,
  repoDraggingRef,
  reorderInFlightRef,
  pendingReorderRef,
}: UseRepoPollArgs): UseRepoPollResult {
  const [repos, setRepos] = useState<Repo[]>([]);
  const [repo, setRepo] = useState<string | null>(null);
  // No highlight is shown until the server supplies its configuration.
  const [hot, setHot] = useState<HotConfig | null>(null);
  // Offset timestamps onto the server clock; unknown until the first response.
  const [clockSkewMs, setClockSkewMs] = useState<number | null>(null);
  // Keeps the empty catalog from flashing before the first session response.
  const [reposLoaded, setReposLoaded] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const controller = new AbortController();
    const refresh = () => {
      const writes = accentWrites.current;
      const widthWrites = sidebarWrites.current;
      const orderGeneration = orderWrites.current;
      return api
        .repos(controller.signal)
        .then(({ repos: list, hot, accent, sidebar_width, now_ms }) => {
          if (cancelled) return;
          setHot(hot);
          setClockSkewMs((held) => nextClockOffset(held, now_ms, Date.now()));
          if (accentWrites.current === writes) adoptAccent(accent);
          if (sidebarWrites.current === widthWrites && !draggingRef.current)
            adoptSidebarWidth(sidebar_width);
          setAuthed(true);
          setReposLoaded(true);
          const reorderPending =
            reorderInFlightRef.current || pendingReorderRef.current !== null;
          if (
            orderWrites.current === orderGeneration &&
            !repoDraggingRef.current &&
            !reorderPending
          ) {
            setRepos(list);
          } else {
            setRepos((current) => {
              const ids = reconcileOrder(
                list.map((item) => item.id),
                current.map((item) => item.id),
              );
              const byId = new Map(list.map((item) => [item.id, item]));
              return ids.map((id) => byId.get(id)!).filter(Boolean);
            });
          }
          setRepo((current) =>
            current && list.some((r) => r.id === current)
              ? current
              : (list[0]?.id ?? null),
          );
          if (!cancelled) timer = setTimeout(refresh, REPO_POLL_MS);
        })
        .catch((err) => {
          if (cancelled) return;
          if (isUnauthorized(err)) {
            setAuthed(false);
            setReposLoaded(false);
          } else if (!isNetworkError(err)) {
            handle(err);
          }
          timer = setTimeout(refresh, REPO_POLL_MS);
        });
    };

    refresh();
    return () => {
      cancelled = true;
      controller.abort();
      if (timer) clearTimeout(timer);
    };
  }, [
    authed,
    setAuthed,
    handle,
    adoptAccent,
    adoptSidebarWidth,
    resumeTick,
    accentWrites,
    sidebarWrites,
    draggingRef,
    orderWrites,
    repoDraggingRef,
    reorderInFlightRef,
    pendingReorderRef,
  ]);

  return {
    repos,
    setRepos,
    repo,
    setRepo,
    hot,
    clockSkewMs,
    reposLoaded,
  };
}
