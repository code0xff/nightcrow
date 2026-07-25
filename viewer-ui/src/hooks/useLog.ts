import { useCallback, useEffect, useRef, useState } from "react";
import { api, type Commit, type CommitFiles } from "../api";

export interface CommitDrillDown extends CommitFiles {
  commit: Commit;
}

export interface UseLogArgs {
  repo: string | null;
  authed: boolean | null;
  tab: "status" | "log" | "tree";
  filter: string;
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
  // Discard responses belonging to a reset log.
  const logRequestRef = useRef(0);
  const resetLog = useCallback(() => {
    logRequestRef.current += 1;
    logLoadingRef.current = false;
    setCommits([]);
    logAnchorRef.current = null;
    setLogDone(false);
    setLogStalled(false);
  }, []);
  const [commitDrillDown, setCommitDrillDown] =
    useState<CommitDrillDown | null>(null);
  const commitsRef = useRef(commits);
  commitsRef.current = commits;

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
    } catch (err) {
      if (request === logRequestRef.current) {
        handle(err);
        setLogStalled(true);
      }
    } finally {
      if (request === logRequestRef.current) logLoadingRef.current = false;
    }
  }, [repo, handle]);

  useEffect(() => {
    if (!repo || !authed || tab !== "log") return;
    if (commits.length === 0 && !logDone && !logStalled) void loadLogPage();
    // Use state here so a repository reset cannot suppress the initial fetch.
  }, [repo, authed, tab, commits.length, logDone, logStalled, loadLogPage]);

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
