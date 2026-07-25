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
  // The commit log, accumulated a page at a time. `logDone` is set once the
  // server reports no more history. Everything below resets together — see
  // `resetLog`.
  const [commits, setCommits] = useState<Commit[]>([]);
  const [logDone, setLogDone] = useState(false);
  // A page failed. Kept apart from `logDone`, which means the history ended:
  // conflating them would report a blip as the end of the log, and the error
  // toast fades on its own, leaving nothing behind to say the list is short.
  // This replaces the sentinel with a retry, which also stops a failing request
  // from firing again on every scroll.
  const [logStalled, setLogStalled] = useState(false);
  // The commit the server walked from, echoed back on every following request
  // so the pages describe one history. A ref, not state: it changes once, when
  // the first page establishes it, and a fetcher rebuilt at that moment would
  // re-arm the paging observer with no new row to justify it.
  const logAnchorRef = useRef<string | null>(null);
  // Guards against two page requests overlapping: the sentinel can re-enter the
  // viewport while a fetch is still out.
  const logLoadingRef = useRef(false);
  // Invalidates a page still in flight when the log it belongs to is discarded
  // (another repo, another tab). Same shape as `paneRequestRef`, kept separate
  // because the two invalidate on different events.
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
  // How many commits are held, read by the page fetcher as the next page's
  // offset. A ref so appending a page does not rebuild the fetcher and re-fire
  // the effect that calls it.
  const commitsRef = useRef(commits);
  commitsRef.current = commits;

  // Fetch one page of the log and append it. The first call (no anchor yet)
  // establishes the anchor from the server's answer; later ones pin to it.
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
      // No anchor to page from (an empty repository) is also the end of it.
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

  // Entering the log tab loads the first page; the sentinel below the list asks
  // for the rest as it comes into view.
  useEffect(() => {
    if (!repo || !authed || tab !== "log") return;
    if (commits.length === 0 && !logDone && !logStalled) void loadLogPage();
    // `commits.length` and not the ref: switching repositories runs this and
    // the effect that empties the list in declaration order, so reading the ref
    // here would see the previous repository's commits, decline to fetch, and
    // leave an empty list nothing would refill — the sentinel that would
    // normally rescue it is not rendered while a filter is up. Depending on the
    // state instead re-runs this once the reset lands, whatever the order.
  }, [repo, authed, tab, commits.length, logDone, logStalled, loadLogPage]);

  // The commit rows the log tab renders. Derived up here, ahead of the sibling
  // list filters below, because the paging observer keys on how many there are.
  const visibleCommits = commits.filter((c) =>
    c.summary.toLowerCase().includes(filter.toLowerCase()),
  );
  // A filter narrows the commits already loaded; it is not a server search. So
  // it also stops the paging, rather than quietly walking the whole history a
  // page at a time hunting for matches — which is what keying the observer on
  // the rendered count alone would still do whenever a page happened to contain
  // one. The list says so where the sentinel would have been.
  const logPagingPaused = filter !== "";

  // Watch the row that sits under the last commit. `rootMargin` starts the
  // fetch a screen early, so scrolling reaches loaded rows rather than the
  // placeholder. The sentinel is only rendered while more history exists, so an
  // exhausted log detaches this instead of polling.
  //
  // Rebuilt whenever the rendered list grows, because an observer reports
  // *changes* in intersection and an appended page need not produce one — the
  // sentinel can stay exactly where it is, in view, with history left to load.
  // Re-observing re-reports the current state, continuing the paging until the
  // sentinel is genuinely pushed out of view.
  //
  // Keyed on the *rendered* count rather than the loaded one, which is what
  // stops a filter from running away with this: a page whose commits the filter
  // hides adds no rows, so it does not re-arm, and the chain stops instead of
  // walking the whole history a page at a time looking for a match. The log
  // filter narrows what is loaded — the same contract the TUI's has.
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