import { useCallback } from "react";
import type { HotConfig, Repo, RepoView } from "../api";
import { otherFace } from "../lib/otherFace";
import { applyTabChoice } from "../lib/tabChoice";
import type { Maximized, Tab } from "../types";
import { useDrillDownEviction } from "./useDrillDownEviction";
import { useRepoData } from "./useRepoData";
import { useRepoPaneActions } from "./useRepoPaneActions";
import { useRepoViewPersistence } from "./useRepoViewPersistence";
import type { ShellLayout } from "./useShellLayout";

export interface RepoProjectContract {
  repo: string | null;
  repos: Repo[];
  authed: boolean | null;
  hot: HotConfig | null;
  clockSkewMs: number | null;
  resumeTick: number;
  handle: (error: unknown) => void;
}

export interface RepoViewContract {
  known: boolean;
  remembered: RepoView | undefined;
  latest: (repo: string) => RepoView | undefined;
  remember: (repo: string | null, view: RepoView) => void;
}

export interface RepoLayoutContract {
  shell: ShellLayout;
  maximizedPanelOf: (repo: string | null) => Maximized;
  setMaximizedFor: (
    repo: string | null,
    next: Maximized | ((previous: Maximized) => Maximized),
  ) => void;
}

interface UseRepoWorkspaceArgs {
  project: RepoProjectContract;
  view: RepoViewContract;
  layout: RepoLayoutContract;
}

/** Assemble the independently testable repository data, pane, and view seams. */
export function useRepoWorkspace({ project, view, layout }: UseRepoWorkspaceArgs) {
  const { repo, repos, authed, hot, clockSkewMs, resumeTick, handle } = project;
  const data = useRepoData({ repo, authed, hot, clockSkewMs, resumeTick, handle });
  const paneActions = useRepoPaneActions({
    repo,
    handle,
    pane: data.screen.pane,
    setPane: data.screen.setPane,
    paneRequestRef: data.request.paneRequestRef,
    setCommitDrillDown: data.log.setCommitDrillDown,
    status: data.status.value,
  });
  const persistence = useRepoViewPersistence({
    repo,
    known: view.known,
    remembered: view.remembered,
    latest: view.latest,
    remember: view.remember,
    setTab: data.screen.setTab,
    clearPane: data.request.clearPane,
    openers: paneActions.openers,
  });

  // The one "show this list" control, assembled here because every part of it
  // lives in this file's seams and none of them in the tab row. The keyboard and
  // the sidebar are handed the same function: a second copy of the sequence is
  // how the keyboard came to record the tab without emptying the pane.
  const showingTab = data.screen.tab;
  const { bumpPaneRequest } = data.request;
  const { setCommitDrillDown, resetLog } = data.log;
  const { chooseTab: recordTab, forgetPane } = persistence;
  const chooseTab = useCallback(
    (next: Tab) =>
      void applyTabChoice(showingTab, next, {
        bumpPaneRequest,
        leaveLog: () => {
          setCommitDrillDown(null);
          resetLog();
        },
        recordTab,
        forgetPane,
      }),
    [
      showingTab,
      bumpPaneRequest,
      setCommitDrillDown,
      resetLog,
      recordTab,
      forgetPane,
    ],
  );

  useDrillDownEviction(
    data.log.commits,
    data.log.commitDrillDown,
    data.log.setCommitDrillDown,
    data.request.bumpPaneRequest,
    persistence.forgetPane,
  );

  const maximized = layout.maximizedPanelOf(repo);
  const setMaximizedFor = layout.setMaximizedFor;
  const setMaximized = useCallback(
    (next: Maximized | ((previous: Maximized) => Maximized)) =>
      setMaximizedFor(repo, next),
    [repo, setMaximizedFor],
  );
  const pane = paneActions.pane;
  const openOtherFace = paneActions.openers.showOtherFace;
  const noteOtherFace = persistence.noteOtherFace;
  const showOtherFace = useCallback(
    (fromHunk: number) => {
      const other = otherFace(pane);
      if (other) {
        noteOtherFace(
          other.source,
          other.want === "file" ? "source" : "diff",
        );
      }
      openOtherFace(pane, fromHunk);
    },
    [pane, noteOtherFace, openOtherFace],
  );

  return {
    setPane: paneActions.setPane,
    setTab: data.screen.setTab,
    clearPane: data.request.clearPane,
    maximized,
    // Exposed for the keyboard, which reaches the same three controls the
    // sidebar and the toolbar do: the composed list chooser the tab row is given
    // below, the tab it would be toggling away from, and the per-project panel
    // maximize already bound to the project on screen.
    tab: showingTab,
    chooseTab,
    setMaximized,
    repoShell: repo
      ? {
          repository: {
            id: repo,
            current: repos.find((candidate) => candidate.id === repo),
            status: data.status.value,
          },
          sidebar: {
            tab: data.screen.tab,
            filter: data.screen.filter,
            setFilter: data.screen.setFilter,
            filterOpen: data.screen.filterOpen,
            setFilterOpen: data.screen.setFilterOpen,
            files: data.status.files,
            now: data.status.now,
            hotWindowMs: data.status.hotWindowMs,
            ...paneActions.openers,
            ...persistence.asked,
            chooseTab,
            authed,
            handle,
            bumpPaneRequest: data.request.bumpPaneRequest,
            ...data.log,
            restoreTree: view.remembered?.tree_expanded ?? [],
            restoreKnown: view.known,
            onTreeExpanded: persistence.noteTree,
            clearPane: persistence.forgetPane,
            touched: persistence.touched,
          },
          filePane: {
            repo,
            pane: paneActions.pane,
            previewRendered: paneActions.previewRendered,
            setPreviewRendered: paneActions.setPreviewRendered,
            showOtherFace,
          },
          layout: {
            ...layout.shell,
            maximized,
            setMaximized,
            mobileView: paneActions.mobileView,
            setMobileView: paneActions.setMobileView,
          },
        }
      : null,
  };
}
