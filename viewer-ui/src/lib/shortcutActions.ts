// The one place a keyboard shortcut, a help row, and a toolbar button agree on
// what they do.
//
// Every command in the web viewer is named here as a semantic action id. The
// key handler resolves a keystroke to an action, the help sheet lists actions,
// and a button dispatches an action — so a rebinding or a new command changes
// one table instead of three call sites, and nothing can drift into naming the
// same command two ways.
//
// The TUI key table lives in Rust (`src/input/routing.rs::prefix_action`) and
// is NOT duplicated here: this table is the web binding of the same *meanings*.
// Where the two must line up — the leader follow-up letters, and the digit row
// `3`..`9`,`0` addressing panes 1..8 — `docs/keybindings.md` is the shared
// statement of record and the only thing both sides are checked against.

export type ShortcutActionId =
  | "terminal.newPane"
  | "terminal.closePane"
  | "terminal.swapPanePrompt"
  | "terminal.claimSizing"
  | "terminal.cancelRecovery"
  | "view.toggleLog"
  | "view.toggleTree"
  | "view.toggleMaximize"
  | "project.openDialog"
  | "project.close"
  | "project.previous"
  | "project.next"
  | "session.cycleAccent"
  | "session.reloadConfig"
  | "focus.list"
  | "focus.content"
  | "focus.pane1"
  | "focus.pane2"
  | "focus.pane3"
  | "focus.pane4"
  | "focus.pane5"
  | "focus.pane6"
  | "focus.pane7"
  | "focus.pane8"
  | "help.shortcuts";

/**
 * How faithfully the web binding reproduces the TUI command.
 *
 * `reinterpreted` is not a lesser binding — it is a promise that the *user*
 * intent survives where the mechanism cannot, and it is surfaced in the help
 * sheet so nobody expects the terminal behaviour byte for byte.
 */
export type ShortcutActionSupport = "direct" | "reinterpreted";

export type ShortcutGroup =
  | "terminal"
  | "project"
  | "view"
  | "focus"
  | "session"
  | "help";

export interface ShortcutAction {
  id: ShortcutActionId;
  label: string;
  /** The label as the hint line prints it, a few words after the key the way
   *  the TUI's hint bar writes `t: new pane`. `label` is the sentence for a
   *  button title and a help row; this is what fits a dozen commands on one
   *  line. */
  hint: string;
  /** The single follow-up key after the leader: one lowercase letter, a digit, or `?`. */
  leader?: string;
  /** A standalone chord in `leaderChord.ts` display form, for actions bound without the leader. */
  chord?: string;
  support: ShortcutActionSupport;
  group: ShortcutGroup;
  note?: string;
  /**
   * True for an action the keyboard is the only way to reach, because pressing
   * it arms a second step rather than running a command: `<prefix> s` waits for
   * a pane digit, and a click has no next key to offer. The help sheet renders
   * such an action as text instead of a button, and its `note` is what says why.
   *
   * Arming a second step is the ONLY legitimate reason a row is not a button.
   * An ordinary action whose row does nothing is a missing handler on the intent
   * bus — a bug — and must be fixed there rather than explained away by setting
   * this flag on it.
   */
  keyboardOnly?: true;
}

// Leader digits for pane focus, mirroring the TUI split-view digit row: `1` and
// `2` address the upper viewer, so panes start at `3` and wrap onto `0` for the
// eighth. This array and `focusPaneNumber` are the ONLY place that numbering is
// written down; everything else asks by action id.
const PANE_FOCUS_DIGITS = ["3", "4", "5", "6", "7", "8", "9", "0"] as const;

const PANE_FOCUS_ACTIONS: ShortcutAction[] = PANE_FOCUS_DIGITS.map(
  (digit, index) => ({
    id: `focus.pane${index + 1}` as ShortcutActionId,
    label: `Focus terminal pane ${index + 1}`,
    hint: `pane ${index + 1}`,
    leader: digit,
    support: "direct" as const,
    group: "focus" as const,
  }),
);

export const SHORTCUT_ACTIONS: readonly ShortcutAction[] = [
  { id: "terminal.newPane", label: "New terminal pane", hint: "new pane", leader: "t", support: "direct", group: "terminal" },
  { id: "terminal.closePane", label: "Close terminal pane", hint: "close pane", leader: "w", support: "direct", group: "terminal" },
  {
    id: "terminal.swapPanePrompt",
    label: "Swap pane with...",
    hint: "swap pane",
    leader: "s",
    support: "direct",
    group: "terminal",
    keyboardOnly: true,
    note: "Arms a second step: the next pane digit picks the pane to swap with, so there is no button for it. Drag a pane to move it instead.",
  },
  { id: "terminal.claimSizing", label: "Claim terminal sizing", hint: "resize panes here", leader: "z", support: "direct", group: "terminal" },
  { id: "terminal.cancelRecovery", label: "Cancel plugin recovery", hint: "cancel recovery", leader: "c", support: "direct", group: "terminal" },
  { id: "view.toggleLog", label: "Toggle status and commit log", hint: "log/status view", leader: "l", support: "direct", group: "view" },
  { id: "view.toggleTree", label: "Toggle tree view", hint: "tree view", leader: "b", support: "direct", group: "view" },
  {
    id: "view.toggleMaximize",
    label: "Maximize panel",
    hint: "maximize",
    leader: "f",
    support: "reinterpreted",
    group: "view",
    // The TUI fullscreen is a redraw inside one terminal window; a page cannot
    // take the browser chrome with it, and F11 belongs to the browser. The
    // intent — "give this panel the whole area" — is kept as the in-page panel
    // maximize and the active-pane terminal zoom. Never F11.
    note: "In the browser this maximizes the panel and zooms the active terminal pane rather than entering OS fullscreen; it is never bound to F11.",
  },
  { id: "project.openDialog", label: "Open project...", hint: "open project", leader: "o", support: "direct", group: "project" },
  { id: "project.close", label: "Close project", hint: "close project", leader: "x", support: "direct", group: "project" },
  {
    id: "project.previous",
    label: "Previous project",
    hint: "prev project",
    chord: "Ctrl+Shift+ArrowLeft",
    support: "direct",
    group: "project",
    note: "Wraps around at the first project.",
  },
  {
    id: "project.next",
    label: "Next project",
    hint: "next project",
    chord: "Ctrl+Shift+ArrowRight",
    support: "direct",
    group: "project",
    note: "Wraps around at the last project.",
  },
  { id: "session.cycleAccent", label: "Cycle session accent", hint: "theme", leader: "p", support: "direct", group: "session" },
  { id: "session.reloadConfig", label: "Reload configuration", hint: "reload config", leader: "u", support: "direct", group: "session" },
  { id: "focus.list", label: "Focus file or commit list", hint: "focus list", leader: "1", support: "direct", group: "focus" },
  { id: "focus.content", label: "Focus content pane", hint: "focus content", leader: "2", support: "direct", group: "focus" },
  ...PANE_FOCUS_ACTIONS,
  { id: "help.shortcuts", label: "Keyboard shortcuts", hint: "shortcuts", leader: "?", support: "direct", group: "help" },
];

/**
 * TUI leader keys the web deliberately leaves unbound, with the reason, so the
 * help sheet and the docs can say so instead of looking incomplete.
 */
export const UNSUPPORTED_TUI_ACTIONS: readonly {
  leader: string;
  label: string;
  reason: string;
}[] = [
  {
    leader: "r",
    label: "Force redraw",
    reason:
      "The browser repaints the page itself; there is no stale frame to force and nothing for the key to do.",
  },
  {
    leader: "q",
    label: "Detach",
    reason:
      "A browser tab is not an attached TUI - closing it already leaves the session running. Signing out is a different, destructive action and is deliberately not bound to a key.",
  },
  {
    leader: "F1-F10",
    label: "Select project tab",
    reason:
      "Bare F-keys are reserved by the browser and the OS, so the web cannot claim them. Use Ctrl+Shift+ArrowLeft / Ctrl+Shift+ArrowRight or the project menu instead.",
  },
];

const BY_LEADER = new Map<string, ShortcutAction>(
  SHORTCUT_ACTIONS.filter((action) => action.leader !== undefined).map(
    (action) => [action.leader as string, action],
  ),
);

const BY_ID = new Map<ShortcutActionId, ShortcutAction>(
  SHORTCUT_ACTIONS.map((action) => [action.id, action]),
);

/**
 * The action a leader follow-up key runs, or null when the key is unmapped.
 *
 * Lowercased before lookup because the TUI `prefix_action` lowercases too: a
 * modifier still held over from the leader chord must not change which command
 * the follow-up names.
 */
export function actionByLeader(key: string): ShortcutAction | null {
  return BY_LEADER.get(key.toLowerCase()) ?? null;
}

export function actionById(id: ShortcutActionId): ShortcutAction {
  const action = BY_ID.get(id);
  // A missing id means the union and the table disagree, which is a build-time
  // mistake worth failing loudly on rather than returning a silent default.
  if (!action) throw new Error(`unknown shortcut action id: ${id}`);
  return action;
}

/** The pane number 1..8 a `focus.paneN` action addresses, else null. */
export function focusPaneNumber(id: ShortcutActionId): number | null {
  const match = /^focus\.pane([1-8])$/.exec(id);
  return match ? Number(match[1]) : null;
}
