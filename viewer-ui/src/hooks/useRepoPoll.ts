import { useEffect, useRef, useState } from "react";
import {
  api,
  isNetworkError,
  isUnauthorized,
  type HotConfig,
  type Repo,
} from "../api";
import { resolveActiveRepo } from "../lib/activeRepo";
import { nextClockOffset } from "../lib/hot";
import { createSerialWriter } from "../lib/serialWrite";
import { reconcileOrder } from "../lib/paneOrder";
import { noteViewerBuild } from "../lib/viewerBuild";
import type { MaximizedByRepo } from "../api";

const REPO_POLL_MS = 3000;

export interface UseRepoPollArgs {
  authed: boolean | null;
  setAuthed: React.Dispatch<React.SetStateAction<boolean | null>>;
  handle: (err: unknown) => void;
  adoptAccent: (accent: number) => void;
  adoptSidebarWidth: (px: number) => void;
  adoptUpperPct: (pct: number) => void;
  adoptMaximized: (remote: MaximizedByRepo) => void;
  draggingRef: React.MutableRefObject<boolean>;
  upperDraggingRef: React.MutableRefObject<boolean>;
  accentWrites: React.MutableRefObject<number>;
  sidebarWrites: React.MutableRefObject<number>;
  upperPctWrites: React.MutableRefObject<number>;
  maximizedWrites: React.MutableRefObject<number>;
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
  /** Whether the server can clone (it has `git` on PATH). */
  canClone: boolean;
}

export function useRepoPoll({
  authed,
  setAuthed,
  handle,
  adoptAccent,
  adoptSidebarWidth,
  adoptUpperPct,
  adoptMaximized,
  draggingRef,
  upperDraggingRef,
  accentWrites,
  sidebarWrites,
  upperPctWrites,
  maximizedWrites,
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
  // Assume cloning works until the server says otherwise, so the form is not
  // briefly disabled on every load.
  const [canClone, setCanClone] = useState(true);
  // One writer for the lifetime of the hook: the queue it holds is what keeps
  // the selection writes in order.
  const { current: writeActiveRepo } = useRef(
    createSerialWriter(api.setActiveRepo),
  );
  // What the session said was in front on the previous poll. The project in
  // front is shared, so a *change* here is another client switching and this
  // page follows it — while an unchanged value is one this page has already
  // adopted, or one its own switch is still writing back.
  const servedActiveRef = useRef<string | null>(null);

  useEffect(() => {
    // `null` performs the initial authentication probe. A successful login
    // changes this back to `true`, which starts a fresh polling effect.
    if (authed === false) return;

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const controller = new AbortController();
    const refresh = () => {
      const writes = accentWrites.current;
      const widthWrites = sidebarWrites.current;
      const splitWrites = upperPctWrites.current;
      const panelWrites = maximizedWrites.current;
      const orderGeneration = orderWrites.current;
      return api
        .repos(controller.signal)
        .then((bootstrap) => {
          const {
            repos: list,
            hot,
            accent,
            sidebar_width,
            upper_pct,
            active_repo,
            maximized,
            now_ms,
            can_clone,
            viewer_build,
          } = bootstrap;
          if (cancelled) return;
          // Not state: what it decides is whether this document is out of date,
          // which nothing here renders. See `lib/viewerBuild.ts`.
          noteViewerBuild(viewer_build);
          setHot(hot);
          setCanClone(can_clone);
          setClockSkewMs((held) => nextClockOffset(held, now_ms, Date.now()));
          if (accentWrites.current === writes) adoptAccent(accent);
          if (sidebarWrites.current === widthWrites && !draggingRef.current)
            adoptSidebarWidth(sidebar_width);
          if (upperPctWrites.current === splitWrites && !upperDraggingRef.current)
            adoptUpperPct(upper_pct);
          if (maximizedWrites.current === panelWrites) adoptMaximized(maximized);
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
          const servedChanged = active_repo !== servedActiveRef.current;
          servedActiveRef.current = active_repo ?? null;
          setRepo((current) =>
            resolveActiveRepo(
              current,
              list.map((r) => r.id),
              active_repo,
              servedChanged,
            ),
          );
          if (!cancelled) timer = setTimeout(refresh, REPO_POLL_MS);
        })
        .catch((err) => {
          if (cancelled) return;
          if (isUnauthorized(err)) {
            setAuthed(false);
            setReposLoaded(false);
            return;
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
    adoptUpperPct,
    resumeTick,
    accentWrites,
    sidebarWrites,
    upperPctWrites,
    maximizedWrites,
    draggingRef,
    upperDraggingRef,
    adoptMaximized,
    orderWrites,
    repoDraggingRef,
    reorderInFlightRef,
    pendingReorderRef,
  ]);

  // Persist the selection from the one place it settles, rather than from each
  // of the four callers that change it (a tab click, the picker, closing a
  // tab, the fallback above) — a caller added later would otherwise be the one
  // that forgets. That includes the fallback: landing somewhere because the
  // old tab closed is still where this page now is, and recording it keeps the
  // server describing a project some client is actually in.
  //
  // Deliberately unconditional, so the first load posts back the very project
  // the server just named. Skipping that write would mean tracking what this
  // page has sent against what the last poll reported, and those two go out of
  // step for a poll every time a write is in flight — long enough for the next
  // switch to read the stale one and skip a write that was needed.
  //
  // Serialized rather than fire-and-forget: two POSTs on separate connections
  // are ordered by arrival, so switching twice quickly could leave the first
  // selection as the one that lands last and sticks.
  useEffect(() => {
    if (!repo) return;
    writeActiveRepo(repo);
  }, [repo, writeActiveRepo]);

  return {
    repos,
    setRepos,
    repo,
    setRepo,
    hot,
    clockSkewMs,
    reposLoaded,
    canClone,
  };
}
