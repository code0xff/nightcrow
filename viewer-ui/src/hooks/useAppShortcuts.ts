import { useCallback, useMemo, useState } from "react";
import { neighborRepo } from "../lib/projectCycle";
import { focusShortcutRegion, terminalPanelHasFocus } from "../lib/shortcutDom";
import type { Maximized, Tab } from "../types";
import {
  useRegisterShortcutHandlers,
  useShortcutIntents,
  type ShortcutHandlers,
} from "./shortcutIntents";
import { useShortcuts } from "./useShortcuts";
import {
  useShortcutSettings,
  type ShortcutSettings,
} from "./useShortcutSettings";

// Where the page's own commands are bound — to the controls the buttons already
// call, never to a second implementation.
//
// Registered rather than called directly, so the keyboard, the help sheet and
// the toolbar all reach the same function, and so "can this run right now" has
// one answer: an action is available exactly when a handler is registered for
// it. That is why the map below is conditional instead of having each handler
// return early — a handler that quietly does nothing would still be advertised
// as available and the help sheet would say the wrong thing.

export interface AppShortcutArgs {
  enabled: boolean;
  repo: string | null;
  repos: readonly { id: string }[];
  selectRepo: (id: string) => void;
  closeRepo: (id: string) => void;
  openPicker: () => void;
  /** The folder picker, which owns the keyboard while it is up. */
  pickerOpen: boolean;
  cycleAccent: () => void;
  reloadConfig: () => void;
  /** The sidebar view on screen. Only meaningful with a project open, which is
   *  what gates the two view commands. */
  tab: Tab;
  chooseTab: (tab: Tab) => void;
  setMaximized: (next: Maximized | ((previous: Maximized) => Maximized)) => void;
}

export interface ShortcutHelp {
  open: boolean;
  show: () => void;
  hide: () => void;
}

export function useAppShortcuts({
  enabled,
  repo,
  repos,
  selectRepo,
  closeRepo,
  openPicker,
  pickerOpen,
  cycleAccent,
  reloadConfig,
  tab,
  chooseTab,
  setMaximized,
}: AppShortcutArgs): {
  shortcutHelp: ShortcutHelp;
  settings: ShortcutSettings;
} {
  const intents = useShortcutIntents();
  const settings = useShortcutSettings();
  const [helpOpen, setHelpOpen] = useState(false);
  const showHelp = useCallback(() => setHelpOpen(true), []);
  const hideHelp = useCallback(() => setHelpOpen(false), []);

  const cycleProject = useCallback(
    (delta: 1 | -1) => {
      const next = neighborRepo(
        repos.map((candidate) => candidate.id),
        repo,
        delta,
      );
      // Nowhere to go is still a claimed key: the chord is reserved wherever the
      // page owns the keyboard, so a one-project session must not be the case
      // where it leaks `ESC[1;6D` into the shell. Consuming is the engine's,
      // which claims every key the registry names.
      if (next !== null) selectRepo(next);
    },
    [repos, repo, selectRepo],
  );

  // `docs/keybindings.md`: the leader `f` maximizes the focused panel and zooms
  // the active terminal pane. The panel with the keyboard is the one it applies
  // to, mirroring the TUI's "fullscreen for the focused panel"; the file pane is
  // the answer everywhere else, because the app chrome belongs to it. Never
  // `requestFullscreen` and never `F11` — see the registry note.
  const zoomActivePane = intents?.zoomActivePane;
  const toggleMaximize = useCallback(() => {
    if (terminalPanelHasFocus()) {
      setMaximized((current) => (current === "terminal" ? "none" : "terminal"));
      zoomActivePane?.();
      return;
    }
    setMaximized((current) => (current === "files" ? "none" : "files"));
  }, [setMaximized, zoomActivePane]);

  const handlers = useMemo<ShortcutHandlers>(() => {
    const map: ShortcutHandlers = {
      "project.openDialog": openPicker,
      "session.cycleAccent": cycleAccent,
      "session.reloadConfig": reloadConfig,
      "help.shortcuts": showHelp,
    };
    if (repos.length > 1) {
      map["project.previous"] = () => cycleProject(-1);
      map["project.next"] = () => cycleProject(1);
    }
    if (repo === null) return map;
    map["project.close"] = () => closeRepo(repo);
    map["view.toggleLog"] = () => chooseTab(tab === "log" ? "status" : "log");
    map["view.toggleTree"] = () => chooseTab(tab === "tree" ? "status" : "tree");
    map["view.toggleMaximize"] = toggleMaximize;
    map["focus.list"] = () => void focusShortcutRegion("list");
    map["focus.content"] = () => void focusShortcutRegion("content");
    return map;
  }, [
    openPicker,
    cycleAccent,
    reloadConfig,
    showHelp,
    repos.length,
    cycleProject,
    repo,
    tab,
    closeRepo,
    chooseTab,
    toggleMaximize,
  ]);

  useRegisterShortcutHandlers(handlers);

  useShortcuts({
    enabled,
    leader: settings.leader,
    // The help sheet is modal too, so the leader does not fire underneath it and
    // the sheet's own keys reach the sheet.
    dialogOpen: pickerOpen || helpOpen,
    repo,
  });

  return {
    shortcutHelp: { open: helpOpen, show: showHelp, hide: hideHelp },
    // The whole of the leader preference, so the help sheet can show the chord,
    // its known collision, and the controls that rebind or switch it off.
    settings,
  };
}
