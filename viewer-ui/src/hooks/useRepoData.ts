import { useCallback, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { HotConfig } from "../api";
import type { Pane, Tab } from "../types";
import { useHotClock } from "./ui/useHotClock";
import { useLog } from "./useLog";
import { useStatus } from "./useStatus";

export interface RepoDataArgs {
  repo: string | null;
  authed: boolean | null;
  hot: HotConfig | null;
  clockSkewMs: number | null;
  resumeTick: number;
  handle: (error: unknown) => void;
}

/** Repository data and screen state that are invalidated together on a switch. */
export function useRepoData({
  repo,
  authed,
  hot,
  clockSkewMs,
  resumeTick,
  handle,
}: RepoDataArgs) {
  const [tab, setTab] = useState<Tab>("status");
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [pane, setPane] = useState<Pane>({ kind: "empty" });
  const [shownRepo, setShownRepo] = useState(repo);
  if (shownRepo !== repo) {
    setShownRepo(repo);
    setPane({ kind: "empty" });
    setTab("status");
  }

  const paneRequestRef = useRef(0);
  const bumpPaneRequest = useCallback(() => {
    paneRequestRef.current += 1;
  }, []);
  const clearPane = useCallback(() => setPane({ kind: "empty" }), []);
  const { status } = useStatus({
    repo,
    authed,
    resumeTick,
    tab,
    pane,
    setPane,
    handle,
    paneRequestRef,
  });
  const hotWindowMs = hot?.enabled ? hot.window_secs * 1000 : 0;
  const now = useHotClock(status?.files, hotWindowMs, clockSkewMs ?? 0);
  const log = useLog({
    repo,
    authed,
    tab,
    filter,
    head: status ? (status.head ?? null) : undefined,
    handle,
  });

  useLayoutEffect(() => {
    bumpPaneRequest();
    log.setCommitDrillDown(null);
    log.resetLog();
  }, [repo, bumpPaneRequest, log.setCommitDrillDown, log.resetLog]);

  const normalizedFilter = filter.toLowerCase();
  const files = useMemo(
    () =>
      (status?.files ?? []).filter((file) =>
        file.path.toLowerCase().includes(normalizedFilter),
      ),
    [status?.files, normalizedFilter],
  );
  const visibleCommitFiles = useMemo(
    () =>
      (log.commitDrillDown?.files ?? []).filter(
        (file) =>
          file.path.toLowerCase().includes(normalizedFilter) ||
          file.old_path?.toLowerCase().includes(normalizedFilter),
      ),
    [log.commitDrillDown?.files, normalizedFilter],
  );
  const aheadOids = useMemo(
    () =>
      new Set(
        log.commits
          .slice(0, status?.tracking?.ahead ?? 0)
          .map((commit) => commit.oid),
      ),
    [log.commits, status?.tracking?.ahead],
  );

  return {
    screen: { tab, setTab, filter, setFilter, filterOpen, setFilterOpen, pane, setPane },
    request: { paneRequestRef, bumpPaneRequest, clearPane },
    status: { value: status, files, now, hotWindowMs },
    log: { ...log, aheadOids, visibleCommitFiles },
  };
}
