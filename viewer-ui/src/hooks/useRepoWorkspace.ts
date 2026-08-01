import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { HotConfig, Repo } from "../api";
import type { Maximized, MobileView, Pane, Tab } from "../types";
import { useHotClock } from "./ui/useHotClock";
import { useLog } from "./useLog";
import { usePaneOpeners } from "./usePaneOpeners";
import type { ShellLayout } from "./useShellLayout";
import { useStatus } from "./useStatus";

interface UseRepoWorkspaceArgs {
  repo: string | null;
  repos: Repo[];
  authed: boolean | null;
  hot: HotConfig | null;
  clockSkewMs: number | null;
  resumeTick: number;
  handle: (error: unknown) => void;
  shell: ShellLayout;
  maximizedPanelOf: (repo: string | null) => Maximized;
  setMaximizedFor: (
    repo: string | null,
    next: Maximized | ((previous: Maximized) => Maximized),
  ) => void;
}

/** State and actions that belong to the repository currently on screen. */
export function useRepoWorkspace({
  repo,
  repos,
  authed,
  hot,
  clockSkewMs,
  resumeTick,
  handle,
  shell,
  maximizedPanelOf,
  setMaximizedFor,
}: UseRepoWorkspaceArgs) {
  const [tab, setTab] = useState<Tab>("status");
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [pane, setPane] = useState<Pane>({ kind: "empty" });
  const [mobileView, setMobileView] = useState<MobileView>("files");
  const [previewRendered, setPreviewRendered] = useState(true);
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
  const maximized = maximizedPanelOf(repo);
  const setMaximized = useCallback(
    (next: Maximized | ((previous: Maximized) => Maximized)) =>
      setMaximizedFor(repo, next),
    [repo, setMaximizedFor],
  );

  const log = useLog({ repo, authed, tab, filter, handle });
  const openers = usePaneOpeners({
    repo,
    handle,
    setPane,
    paneRequestRef,
    setCommitDrillDown: log.setCommitDrillDown,
    setMobileView,
    setPreviewRendered,
  });

  // A repository switch invalidates every view tied to the one being left.
  useLayoutEffect(() => {
    bumpPaneRequest();
    log.setCommitDrillDown(null);
    clearPane();
    log.resetLog();
  }, [repo, bumpPaneRequest, clearPane, log.setCommitDrillDown, log.resetLog]);

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
    setPane,
    setTab,
    clearPane,
    maximized,
    repoShell: repo
      ? {
          repository: {
            id: repo,
            current: repos.find((candidate) => candidate.id === repo),
            status,
          },
          sidebar: {
            tab,
            setTab,
            filter,
            setFilter,
            filterOpen,
            setFilterOpen,
            files,
            now,
            hotWindowMs,
            setPane,
            ...openers,
            authed,
            handle,
            bumpPaneRequest,
            ...log,
            aheadOids,
            visibleCommitFiles,
          },
          filePane: {
            pane,
            previewRendered,
            setPreviewRendered,
            // Bound to the pane here rather than in the component, which has no
            // business knowing what a pane is made of.
            showOtherFace: () => openers.showOtherFace(pane),
          },
          layout: {
            ...shell,
            maximized,
            setMaximized,
            mobileView,
            setMobileView,
          },
        }
      : null,
  };
}
