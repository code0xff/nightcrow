import { useCallback, useState } from "react";
import { isUnauthorized } from "../api";
import { appRows } from "../layout/appLayout";
import { toast } from "../lib/toast";
import { useClone } from "./useClone";
import { useProjectTabs } from "./useProjectTabs";
import { useRepoActions } from "./useRepoActions";
import { useRepoWorkspace } from "./useRepoWorkspace";
import { useResumeTick } from "./useResumeTick";
import { useShellLayout } from "./useShellLayout";

/** The page-level seams between authentication, projects, and repository UI. */
export function useAppViewModel() {
  const [authed, setAuthed] = useState<boolean | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const handle = useCallback((error: unknown) => {
    if (isUnauthorized(error)) {
      setAuthed(false);
      return;
    }
    toast.error(error instanceof Error ? error.message : "request failed");
  }, []);
  const resumeTick = useResumeTick();
  const layout = useShellLayout();

  const tabs = useProjectTabs({
    authed,
    setAuthed,
    handle,
    resumeTick,
    ...layout.guards,
  });
  const workspace = useRepoWorkspace({
    repo: tabs.repo,
    repos: tabs.repos,
    authed,
    hot: tabs.hot,
    clockSkewMs: tabs.clockSkewMs,
    resumeTick,
    handle,
    shell: layout.shell,
    maximizedPanelOf: layout.maximizedPanelOf,
    setMaximizedFor: layout.setMaximizedFor,
  });

  const { selectOpenedRepo, closeRepo } = useRepoActions({
    repos: tabs.repos,
    setRepos: tabs.setRepos,
    setRepo: tabs.setRepo,
    setPane: workspace.setPane,
    setTab: workspace.setTab,
    setPickerOpen,
    handle,
    orderWrites: tabs.orderWrites,
  });
  // A clone outlives the picker and the page that started it can be reloaded.
  const { busy: cloning, start: startClone } = useClone(
    selectOpenedRepo,
    authed === true,
  );

  const selectRepo = useCallback(
    (id: string) => {
      tabs.setRepo(id);
      workspace.clearPane();
    },
    [tabs.setRepo, workspace.clearPane],
  );
  const openPicker = useCallback(() => setPickerOpen(true), []);
  const closePicker = useCallback(() => setPickerOpen(false), []);
  // Re-bootstrap after login instead of mounting repository state retained
  // from an expired session before the first authenticated poll completes.
  const login = useCallback(() => setAuthed(null), []);

  return {
    authed,
    login,
    reposLoaded: tabs.reposLoaded,
    rows: appRows(tabs.repo, workspace.maximized),
    upperPct: layout.upperPct,
    header: {
      repos: tabs.repos,
      repo: tabs.repo,
      onSelectRepo: selectRepo,
      onCloseRepo: closeRepo,
      onOpenPicker: openPicker,
      cloning,
      accent: layout.accent,
      next: layout.next,
      cycle: layout.cycle,
      draggingRepo: tabs.draggingRepo,
      dragOverRepo: tabs.dragOverRepo,
      onRepoDragStart: tabs.onRepoDragStart,
      onRepoDragMove: tabs.onRepoDragMove,
      onRepoDragEnd: tabs.onRepoDragEnd,
    },
    repoShell: workspace.repoShell,
    picker: pickerOpen
      ? {
          onClose: closePicker,
          onOpened: selectOpenedRepo,
          canClone: tabs.canClone,
          cloning,
          onClone: startClone,
        }
      : null,
  };
}
