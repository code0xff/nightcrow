// Choosing a sidebar list, as one operation.
//
// Setting the tab is the smallest part of it. The list being left owns the
// content pane beside it and, for the log, a history snapshot and a drill-down —
// so a switch that only records the tab leaves the previous list's diff on
// screen, lets a pane request already in flight land after the switch, and
// re-enters the log on a stale page. The TUI does all of it in one place
// (`src/app/focus.rs::toggle_mode`, which clears the diff and resets the
// drill-down), and this is the web's.
//
// Pure: the caller supplies the four operations and this decides whether and in
// which order they run, so the tab row and the keyboard cannot each grow their
// own version of the sequence.

import type { Tab } from "../types";

export interface TabChoiceOps {
  /** Invalidate pane requests in flight, so a reply asked for by the list being
   *  left cannot land in the pane after the switch. */
  bumpPaneRequest: () => void;
  /** Drop the commit drill-down and the cached history: re-entering the log must
   *  use a fresh snapshot. */
  leaveLog: () => void;
  /** Record the choice and show the list. */
  recordTab: (next: Tab) => void;
  /** Empty the content pane the previous list filled, and forget it. */
  forgetPane: () => void;
}

/**
 * Apply one tab choice, reporting whether anything happened.
 *
 * Choosing the list already showing is not a change: emptying its pane for a
 * tap that asked for nothing would throw away what the person is reading.
 */
export function applyTabChoice(
  current: Tab,
  next: Tab,
  ops: TabChoiceOps,
): boolean {
  if (next === current) return false;
  ops.bumpPaneRequest();
  if (current === "log") ops.leaveLog();
  ops.recordTab(next);
  ops.forgetPane();
  return true;
}
