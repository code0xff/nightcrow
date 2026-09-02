import { useCallback, useState } from "react";
import { isUnauthorized } from "../api";
import { appRows } from "../layout/appLayout";
import { toast } from "../lib/toast";
import { useAppShortcuts } from "./useAppShortcuts";
import { useClone } from "./useClone";
import { useProjectTabs } from "./useProjectTabs";
import { useReloadConfig } from "./useReloadConfig";
import { useRepoActions } from "./useRepoActions";
import { useRepoWorkspace } from "./useRepoWorkspace";
import { useResumeTick } from "./useResumeTick";
import { useShellLayout } from "./useShellLayout";
import { useTabStripSide } from "./ui/tabStripSide";

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
  // This device's alone, like the pane view mode; see the hook.
  const tabStrip = useTabStripSide();

  const tabs = useProjectTabs({
    authed,
    setAuthed,
    handle,
    resumeTick,
    ...layout.guards,
  });
  const workspace = useRepoWorkspace({
    project: {
      repo: tabs.repo,
      repos: tabs.repos,
      authed,
      hot: tabs.hot,
      clockSkewMs: tabs.clockSkewMs,
      resumeTick,
      handle,
    },
    view: {
      known: layout.viewCovers(tabs.repo),
      remembered: layout.viewOf(tabs.repo),
      latest: layout.rememberedViewFor,
      remember: layout.rememberView,
    },
    layout: {
      shell: layout.shell,
      maximizedPanelOf: layout.maximizedPanelOf,
      setMaximizedFor: layout.setMaximizedFor,
    },
  });

  const { selectOpenedRepo, closeRepo } = useRepoActions({
    repo: tabs.repo,
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
      // Choosing the project already on screen is not a change: clearing its
      // pane would leave the screen saying one thing and its record another,
      // for a tap that asked for nothing.
      if (id === tabs.repo) return;
      tabs.setRepo(id);
      workspace.clearPane();
    },
    [tabs.repo, tabs.setRepo, workspace.clearPane],
  );
  const openPicker = useCallback(() => setPickerOpen(true), []);
  const closePicker = useCallback(() => setPickerOpen(false), []);
  // Hoisted out of `Header` so the button and the keyboard share one reload:
  // a second instance would keep its own `inFlight` guard and two requests
  // could be open at once.
  const config = useReloadConfig();
  // The whole keyboard, mounted here because this is where everything it drives
  // already meets: authentication, the picker, the tab order and the one
  // selection path. Going through `selectRepo` rather than writing the active
  // project itself is what keeps a shortcut switch identical to a tab click —
  // pane clear, per-project view restore, and the single write-back in
  // `useRepoPoll` that the `adoptedRef` invariant depends on.
  const shortcuts = useAppShortcuts({
    enabled: authed === true,
    repo: tabs.repo,
    repos: tabs.repos,
    selectRepo,
    closeRepo,
    openPicker,
    pickerOpen,
    cycleAccent: layout.cycle,
    reloadConfig: config.reload,
    tab: workspace.tab,
    chooseTab: workspace.chooseTab,
    maximized: workspace.maximized,
    setMaximized: workspace.setMaximized,
    mobileView: workspace.repoShell?.layout.mobileView ?? "files",
  });
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
      onReloadConfig: config.reload,
      reloading: config.pending,
      tabStrip,
    },
    // Where the project strip is drawn. The page places the left one itself,
    // outside the grid the header tops; the header draws the top one.
    tabStrip,
    // The help sheet's state only. Another component renders it, and it reads
    // what is available and what the leader is from the same two sources the
    // keyboard does — the intent bus and `useShortcutSettings`.
    shortcutHelp: shortcuts.shortcutHelp,
    leader: shortcuts.settings,
    hint: shortcuts.hint,
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
