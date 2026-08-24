import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type { Commit, HotConfig, Repo } from "../api";
import type { Maximized, MobileView, Pane, Tab } from "../types";
import { useHotClock } from "./ui/useHotClock";
import { useDrillDownEviction } from "./useDrillDownEviction";
import { useLog } from "./useLog";
import { usePaneOpeners } from "./usePaneOpeners";
import type { ShellLayout } from "./useShellLayout";
import { useRepoViewMemory } from "./useRepoViewMemory";
import { commitFile, workdirFile } from "../lib/repoView";
import { otherFace } from "../lib/otherFace";
import { useStatus } from "./useStatus";
import type { RepoView } from "../api";

interface UseRepoWorkspaceArgs {
  repo: string | null;
  repos: Repo[];
  authed: boolean | null;
  hot: HotConfig | null;
  clockSkewMs: number | null;
  resumeTick: number;
  handle: (error: unknown) => void;
  shell: ShellLayout;
  /** Whether the server has answered about this project yet. */
  viewKnown: boolean;
  /** What this project was last showing, and where to record it now. */
  rememberedView: RepoView | undefined;
  latestView: (repo: string) => RepoView | undefined;
  rememberView: (repo: string | null, view: RepoView) => void;
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
  viewKnown,
  rememberedView,
  latestView,
  rememberView,
  maximizedPanelOf,
  setMaximizedFor,
}: UseRepoWorkspaceArgs) {
  const [tab, setTab] = useState<Tab>("status");
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [pane, setPane] = useState<Pane>({ kind: "empty" });
  const [mobileView, setMobileView] = useState<MobileView>("files");
  const [previewRendered, setPreviewRendered] = useState(true);
  // What the state above belongs to. A project change is applied *during this
  // render* rather than from an effect: an effect leaves one render in which
  // the pane and the tab are still the project just left, and everything
  // reading them then — the view memory above all — has to be told to
  // disbelieve what it is looking at. React re-renders with these before
  // committing, so that render never happens.
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
  const maximized = maximizedPanelOf(repo);
  const setMaximized = useCallback(
    (next: Maximized | ((previous: Maximized) => Maximized)) =>
      setMaximizedFor(repo, next),
    [repo, setMaximizedFor],
  );

  // Three-valued on purpose: no status yet is not knowing, while a status
  // without a head is knowing the server could not name one (unborn HEAD, or
  // one it could not read) — the log must react to the second and hold still
  // for the first.
  const log = useLog({
    repo,
    authed,
    tab,
    filter,
    head: status ? (status.head ?? null) : undefined,
    handle,
  });
  // Read by the pane openers at the moment they act, so "does this still have a
  // working copy" is answered from now rather than from whenever a callback was
  // built.
  const statusRef = useRef(status);
  statusRef.current = status;
  const openers = usePaneOpeners({
    repo,
    handle,
    setPane,
    paneRequestRef,
    setCommitDrillDown: log.setCommitDrillDown,
    setMobileView,
    setPreviewRendered,
    statusRef,
  });

  const memory = useRepoViewMemory({
    repo,
    known: viewKnown,
    remembered: rememberedView,
    latest: latestView,
    remember: rememberView,
    setTab,
    openDiff: openers.openDiff,
    openFile: openers.openFile,
    openCommitFileDiff: openers.openCommitFileDiff,
  });

  // Every way to change what this project is showing, each recording the choice
  // it *is* rather than leaving the record to work it out from the screen
  // afterwards (`useRepoViewMemory`).
  const { noteFile, noteTab, noteTree } = memory;
  const chooseTab = useCallback(
    (next: Tab) => {
      noteTab(next);
      setTab(next);
    },
    [noteTab],
  );
  // Emptying the pane on purpose — out of a commit's file list — is a choice
  // too, and the only one that is not an opener.
  const forgetPane = useCallback(() => {
    noteFile(null);
    clearPane();
  }, [noteFile, clearPane]);
  const asked = useMemo(
    () => ({
      openDiff: (path: string) => {
        noteFile(workdirFile(path, "diff"));
        openers.openDiff(path);
      },
      openFile: (path: string) => {
        noteFile(workdirFile(path, "source"));
        openers.openFile(path);
      },
      // A whole commit's diff spans several files, so no single one names it.
      openCommit: (oid: string) => {
        noteFile(null);
        openers.openCommit(oid);
      },
      openCommitFiles: (commit: Commit) => {
        noteFile(null);
        return openers.openCommitFiles(commit);
      },
      openCommitFileDiff: (oid: string, path: string) => {
        noteFile(commitFile(oid, path, "diff"));
        openers.openCommitFileDiff(oid, path);
      },
    }),
    [openers, noteFile],
  );

  // The rest of what a repository switch invalidates. The screen's own state is
  // reset above, during the render; these belong to other hooks and to a ref,
  // and none of them is read as "what this project is showing".
  useLayoutEffect(() => {
    bumpPaneRequest();
    log.setCommitDrillDown(null);
    log.resetLog();
  }, [repo, bumpPaneRequest, log.setCommitDrillDown, log.resetLog]);

  useDrillDownEviction(
    log.commits,
    log.commitDrillDown,
    log.setCommitDrillDown,
    bumpPaneRequest,
    forgetPane,
  );

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
            filter,
            setFilter,
            filterOpen,
            setFilterOpen,
            files,
            now,
            hotWindowMs,
            ...openers,
            ...asked,
            setTab: chooseTab,
            authed,
            handle,
            bumpPaneRequest,
            ...log,
            aheadOids,
            visibleCommitFiles,
            // The tree's half of the remembered view: the shape to put it back
            // into, and where the shape it ends up in is reported back to.
            restoreTree: rememberedView?.tree_expanded ?? [],
            restoreKnown: viewKnown,
            onTreeExpanded: noteTree,
            clearPane: forgetPane,
            touched: memory.touched,
          },
          filePane: {
            repo,
            pane,
            previewRendered,
            setPreviewRendered,
            // Bound to the pane here rather than in the component, which has no
            // business knowing what a pane is made of.
            showOtherFace: (fromHunk: number) => {
              // The same file, its other face — which is what the pane's own
              // source says, and what the opener is about to fetch.
              const other = otherFace(pane);
              if (other) {
                const face = other.want === "file" ? "source" : "diff";
                noteFile(
                  other.source.kind === "commit"
                    ? commitFile(other.source.oid, other.source.path, face)
                    : workdirFile(other.source.path, face),
                );
              }
              openers.showOtherFace(pane, fromHunk);
            },
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
