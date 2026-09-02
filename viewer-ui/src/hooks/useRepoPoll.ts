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
import { retainHot, retainRepos } from "../lib/repoSnapshot";
import { noteViewerBuild } from "../lib/viewerBuild";
import type { MaximizedByRepo, RepoViewByRepo } from "../api";

const REPO_POLL_MS = 3000;

export interface UseRepoPollArgs {
  authed: boolean | null;
  setAuthed: React.Dispatch<React.SetStateAction<boolean | null>>;
  handle: (err: unknown) => void;
  adoptAccent: (accent: number) => void;
  adoptUpperPct: (pct: number) => void;
  adoptMaximized: (remote: MaximizedByRepo) => void;
  adoptViews: (remote: RepoViewByRepo, served: string[]) => void;
  upperDraggingRef: React.MutableRefObject<boolean>;
  accentWrites: React.MutableRefObject<number>;
  upperPctWrites: React.MutableRefObject<number>;
  maximizedWrites: React.MutableRefObject<number>;
  viewWrites: React.MutableRefObject<number>;
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
  adoptUpperPct,
  adoptMaximized,
  adoptViews,
  upperDraggingRef,
  accentWrites,
  upperPctWrites,
  maximizedWrites,
  viewWrites,
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
  // The value the poll most recently made this page follow, held so the write
  // effect below can tell a followed value from a chosen one. A value rather
  // than a flag: state updates batch, and a boolean set by the poll could
  // still be standing when a person's own switch becomes the final value of
  // the same render — silently swallowing the one write that mattered. The
  // value survives that: it only ever matches what the poll itself set.
  const adoptedRef = useRef<string | null>(null);

  useEffect(() => {
    // `null` performs the initial authentication probe. A successful login
    // changes this back to `true`, which starts a fresh polling effect.
    if (authed === false) return;

    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const controller = new AbortController();
    const refresh = () => {
      const writes = accentWrites.current;
      const splitWrites = upperPctWrites.current;
      const panelWrites = maximizedWrites.current;
      const lastViewWrites = viewWrites.current;
      const orderGeneration = orderWrites.current;
      return api
        .repos(controller.signal)
        .then((bootstrap) => {
          const {
            repos: list,
            hot,
            accent,
            upper_pct,
            active_repo,
            maximized,
            last_view,
            now_ms,
            can_clone,
            viewer_build,
          } = bootstrap;
          if (cancelled) return;
          // Not state: what it decides is whether this document is out of date,
          // which nothing here renders. See `lib/viewerBuild.ts`.
          noteViewerBuild(viewer_build);
          setHot((current) => retainHot(current, hot));
          setCanClone(can_clone);
          setClockSkewMs((held) => nextClockOffset(held, now_ms, Date.now()));
          if (accentWrites.current === writes) adoptAccent(accent);
          if (upperPctWrites.current === splitWrites && !upperDraggingRef.current)
            adoptUpperPct(upper_pct);
          if (maximizedWrites.current === panelWrites) adoptMaximized(maximized);
          // `?? {}` because an older server does not send the field at all,
          // and the map's absence must not stop the page from loading.
          if (viewWrites.current === lastViewWrites)
            adoptViews(last_view ?? {}, list.map((item) => item.id));
          setAuthed(true);
          setReposLoaded(true);
          const reorderPending =
            reorderInFlightRef.current || pendingReorderRef.current !== null;
          if (
            orderWrites.current === orderGeneration &&
            !repoDraggingRef.current &&
            !reorderPending
          ) {
            setRepos((current) => retainRepos(current, list));
          } else {
            setRepos((current) => {
              const ids = reconcileOrder(
                list.map((item) => item.id),
                current.map((item) => item.id),
              );
              const byId = new Map(list.map((item) => [item.id, item]));
              return retainRepos(
                current,
                ids.map((id) => byId.get(id)!).filter(Boolean),
              );
            });
          }
          const ids = list.map((r) => r.id);
          const servedChanged = active_repo !== servedActiveRef.current;
          servedActiveRef.current = active_repo ?? null;
          // A switch onto the *served* value is the page following the
          // session, not the person at this page choosing — marked so the
          // write below stays quiet about it. Landing anywhere else (the
          // first-tab fallback when nothing served resolves) is this page's
          // own doing and is still written, which keeps the server describing
          // a project some client is actually in.
          //
          // Decided here rather than inside the state updater: an updater can
          // run for a render that never commits, and a mark left behind by one
          // would suppress a real write later. The decision needs no
          // `current` — a changed served value that is open wins in
          // `resolveActiveRepo` regardless of it.
          if (servedChanged && active_repo && ids.includes(active_repo)) {
            adoptedRef.current = active_repo;
          }
          setRepo((current) =>
            resolveActiveRepo(current, ids, active_repo, servedChanged),
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
    adoptUpperPct,
    resumeTick,
    accentWrites,
    upperPctWrites,
    maximizedWrites,
    viewWrites,
    upperDraggingRef,
    adoptMaximized,
    adoptViews,
    orderWrites,
    repoDraggingRef,
    reorderInFlightRef,
    pendingReorderRef,
  ]);

  // Persist the selection from the one place it settles, rather than from each
  // of the callers that change it (a tab click, the picker, closing a tab) —
  // a caller added later would otherwise be the one that forgets.
  //
  // But only this page's own switches are written. A value the poll adopted
  // is the session talking, and echoing it back turned two open pages into a
  // feedback loop: each page followed the other's write, wrote back what it
  // had just followed, and the front repository oscillated between them once
  // a second for as long as both pages lived — every flip tearing both
  // terminal panels down mid-replay, which a person saw as panes with no
  // history. The session already knows an adopted value; only a choice made
  // at this page is news.
  //
  // This is not "skip when it matches the server", which was tried and
  // reverted (A→B→A inside one poll left B on the server): the mark is the
  // one value the poll set, and a person's write clears it, so returning to
  // a followed project is still recorded.
  //
  // Serialized rather than fire-and-forget: two POSTs on separate connections
  // are ordered by arrival, so switching twice quickly could leave the first
  // selection as the one that lands last and sticks.
  useEffect(() => {
    if (!repo) {
      // Losing the selection outlives any adoption: without this, a mark left
      // from before every tab closed could match the first-tab fallback of a
      // later reopen and swallow the write that should record it.
      adoptedRef.current = null;
      return;
    }
    if (repo === adoptedRef.current) return;
    adoptedRef.current = null;
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
