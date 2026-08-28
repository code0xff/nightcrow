import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Commit, type CommitFiles } from "../api";
import { reconcileLog } from "../lib/logRefresh";

export interface CommitDrillDown extends CommitFiles {
  commit: Commit;
}

export interface UseLogArgs {
  repo: string | null;
  authed: boolean | null;
  tab: "status" | "log" | "tree";
  filter: string;
  /** The repository's HEAD as the status stream last reported it. A change
   *  while the log tab is open refreshes the list (`lib/logRefresh.ts`).
   *  `undefined` while no status has arrived; `null` when one has and could
   *  not name a head — unborn (an empty repository, an orphan checkout) or
   *  unreadable, which the refresh then surfaces as its own error. A detached
   *  HEAD still reports its commit. */
  head: string | null | undefined;
  handle: (err: unknown) => void;
}

export interface UseLogResult {
  commits: Commit[];
  logDone: boolean;
  logStalled: boolean;
  setLogStalled: (v: boolean | ((prev: boolean) => boolean)) => void;
  commitDrillDown: CommitDrillDown | null;
  setCommitDrillDown: (v: CommitDrillDown | null) => void;
  resetLog: () => void;
  logSentinelRef: React.RefObject<HTMLLIElement | null>;
  visibleCommits: Commit[];
  logPagingPaused: boolean;
}

export function useLog({
  repo,
  authed,
  tab,
  filter,
  head,
  handle,
}: UseLogArgs): UseLogResult {
  const [commits, setCommits] = useState<Commit[]>([]);
  const [logDone, setLogDone] = useState(false);
  // Keep transport failure distinct from end-of-history so the list can offer retry.
  const [logStalled, setLogStalled] = useState(false);
  // Pin later pages to the same server-side history snapshot.
  const logAnchorRef = useRef<string | null>(null);
  // The sentinel can re-enter while a page request is still pending.
  const logLoadingRef = useRef(false);
  // Distinct from `logLoadingRef` (which paging also holds): whether a
  // *refresh* is in flight, because only a refresh carries an ask to protect.
  const refreshingRef = useRef(false);
  // Discard responses belonging to a reset log.
  const logRequestRef = useRef(0);
  // The head the cached pages were walked from — the fact a refresh decision
  // compares against, rather than a "previous head" baseline, which loses the
  // moves that happen while nothing is loaded yet (an empty repository's first
  // commit, a commit landing during the initial fetch). `undefined` until a
  // first page lands, `null` when that page said the history was empty.
  const accountedHeadRef = useRef<string | null | undefined>(undefined);
  // The head the last refresh was asked for. The walk can return history
  // *newer* than the status report that asked for it (the stream lags the
  // repository), leaving a standing disagreement no further fetch resolves —
  // this mark keeps that from becoming a fetch loop. A failed ask clears it,
  // so the retry can.
  const askedHeadRef = useRef<string | null | undefined>(undefined);
  const resetLog = useCallback(() => {
    logRequestRef.current += 1;
    logLoadingRef.current = false;
    refreshingRef.current = false;
    setCommits([]);
    logAnchorRef.current = null;
    setLogDone(false);
    setLogStalled(false);
    accountedHeadRef.current = undefined;
    askedHeadRef.current = undefined;
  }, []);
  const [commitDrillDown, setCommitDrillDown] =
    useState<CommitDrillDown | null>(null);
  const commitsRef = useRef(commits);
  commitsRef.current = commits;
  const logDoneRef = useRef(logDone);
  logDoneRef.current = logDone;
  const headRef = useRef(head);
  headRef.current = head;

  // The head moved under an open log: fetch a fresh first page and fold it
  // onto the cache (`reconcileLog`). Bumping the generation first discards any
  // page fetch still in flight — it was asked against history that no longer
  // matches. The TUI cancels its fetch worker at the same point. A failure
  // leaves `accountedHeadRef` where it was, so the disagreement that asked for
  // this refresh is still standing when the retry row clears `logStalled`.
  //
  // `seed` is for a caller whose cache has not rendered yet — the refs below
  // lag one render, which the first-page landing beats (the TUI captures its
  // pre-spawn state the same way).
  const refreshLogPage = useCallback(
    async (seed?: { commits: Commit[]; done: boolean }) => {
      if (!repo) return;
      askedHeadRef.current = headRef.current;
      logRequestRef.current += 1;
      const request = logRequestRef.current;
      logLoadingRef.current = true;
      refreshingRef.current = true;
      try {
        const fresh = await api.log(repo);
        if (request !== logRequestRef.current) return;
        const cache = seed ?? {
          commits: commitsRef.current,
          done: logDoneRef.current,
        };
        const next = reconcileLog(fresh, cache.commits, cache.done);
        setCommits(next.commits);
        logAnchorRef.current = next.anchor;
        accountedHeadRef.current = next.anchor;
        // An answer that agrees with the report spends the ask; one that
        // outran it keeps the mark, which is the loop guard.
        if (next.anchor === headRef.current) askedHeadRef.current = undefined;
        setLogDone(next.done);
        setLogStalled(false);
      } catch (err) {
        if (request === logRequestRef.current) {
          askedHeadRef.current = undefined;
          handle(err);
          setLogStalled(true);
        }
      } finally {
        if (request === logRequestRef.current) {
          logLoadingRef.current = false;
          refreshingRef.current = false;
        }
      }
    },
    [repo, handle],
  );

  const loadLogPage = useCallback(async () => {
    if (!repo || logLoadingRef.current) return;
    logLoadingRef.current = true;
    const request = logRequestRef.current;
    try {
      const anchor = logAnchorRef.current;
      const page = await api.log(
        repo,
        anchor === null
          ? undefined
          : { from: anchor, skip: commitsRef.current.length },
      );
      if (request !== logRequestRef.current) return;
      setCommits((held) => [...held, ...page.commits]);
      logAnchorRef.current = page.head ?? null;
      setLogDone(!page.truncated || page.head === undefined);
      if (anchor === null) {
        accountedHeadRef.current = page.head ?? null;
        // The status stream can move past a first page while it is in flight;
        // its report outranks the page (it is newer), so the disagreement is
        // resolved the way any other head move is. The page seeds the refresh
        // because it has not rendered into the refs yet.
        const reported = headRef.current;
        if (reported !== undefined && reported !== (page.head ?? null)) {
          void refreshLogPage({
            commits: page.commits,
            done: !page.truncated || page.head === undefined,
          });
        }
      }
    } catch (err) {
      if (request === logRequestRef.current) {
        handle(err);
        setLogStalled(true);
      }
    } finally {
      if (request === logRequestRef.current) logLoadingRef.current = false;
    }
  }, [repo, handle, refreshLogPage]);

  useEffect(() => {
    if (!repo || !authed || tab !== "log") return;
    if (commits.length === 0 && !logDone && !logStalled) void loadLogPage();
    // Use state here so a repository reset cannot suppress the initial fetch.
  }, [repo, authed, tab, commits.length, logDone, logStalled, loadLogPage]);

  // Refresh when the status stream and the cache disagree about the head, and
  // only while the log is on screen — leaving the tab resets the log anyway.
  // Before a first page lands there is nothing to disagree with (its landing
  // re-checks, above). `undefined` — no status yet — is silence, not a move;
  // `null` is a report like any oid (the branch became unborn, and a list of
  // commits over no head is as stale as one behind it). While stalled the
  // retry row owns the next attempt: clearing it re-runs this comparison, so
  // a failed refresh is retried as a refresh.
  useEffect(() => {
    if (!repo || !authed || tab !== "log" || head === undefined) return;
    if (logStalled) return;
    const accounted = accountedHeadRef.current;
    if (accounted === undefined) return;
    if (accounted === head) {
      // Agreement spends any standing ask: the stream has caught up with the
      // walk that outran it, and the same head reported again later would be
      // a real move (a reset back), not the race replaying. Not while a
      // refresh is in flight, though — this agreement is with the cache that
      // refresh is about to replace, and its ask still guards its answer.
      if (!refreshingRef.current) askedHeadRef.current = undefined;
      return;
    }
    // Already asked and answered: the walk outran the report (see
    // `askedHeadRef`), and re-asking would only fetch the same newer history.
    if (askedHeadRef.current === head) return;
    void refreshLogPage();
  }, [repo, authed, tab, head, logStalled, refreshLogPage]);

  const visibleCommits = commits.filter((c) =>
    c.summary.toLowerCase().includes(filter.toLowerCase()),
  );
  // Filtering is client-side; do not page through the entire history looking for matches.
  const logPagingPaused = filter !== "";

  // Re-observe when rendered rows grow; intersection observers do not report an unchanged sentinel again.
  const logSentinelRef = useRef<HTMLLIElement>(null);
  useEffect(() => {
    const sentinel = logSentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) void loadLogPage();
      },
      { root: sentinel.closest("ul"), rootMargin: "400px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [
    loadLogPage,
    logDone,
    logStalled,
    logPagingPaused,
    commitDrillDown,
    tab,
    visibleCommits.length,
  ]);

  return {
    commits,
    logDone,
    logStalled,
    setLogStalled,
    commitDrillDown,
    setCommitDrillDown,
    resetLog,
    logSentinelRef,
    visibleCommits,
    logPagingPaused,
  };
}
