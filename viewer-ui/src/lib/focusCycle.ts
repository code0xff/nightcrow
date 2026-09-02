// Where `Shift+Left` / `Shift+Right` send the keyboard.
//
// The TUI walks one ring (`src/app/focus.rs::cycle_focus_forward`): the file
// list, the content pane, then each terminal pane in order, and round to the
// list again. The web binds the same meaning. What the two sides share is that
// ring; what differs is how each knows what is on screen — the TUI has its
// fullscreen flags, the page has a maximized panel and, below `md`, one chosen
// view — so the ring is built here from what is showing, and neither side's
// flags are copied into the other.
//
// Pure and DOM-free: the page reads where the keyboard is and how many panes
// there are; this says where it goes next.

import type { Maximized, MobileView } from "../types";

export type FocusSpot =
  | { kind: "list" }
  | { kind: "content" }
  | { kind: "pane"; index: number };

export interface FocusRing {
  /** Where the keyboard is, or null when nothing on the ring holds it — the
   *  header, the footer, the body after a click on nothing. */
  at: FocusSpot | null;
  paneCount: number;
  maximized: Maximized;
  /** Below the `md` breakpoint, where `RepoShell` shows one view at a time. */
  narrow: boolean;
  /** The view a narrow screen is showing; read only when `narrow`. */
  mobileView: MobileView;
}

/**
 * The spot the keyboard moves to, or null when there is nowhere to go.
 *
 * Only what is on screen is on the ring. The TUI's rule reads as three cases —
 * nothing in list or diff fullscreen, panes only in terminal fullscreen, the
 * whole ring otherwise — but they are one rule, cycle what is showing, and that
 * is what maps onto the page: a maximized terminal shows its panes, a maximized
 * upper region shows the list and the content pane, and a narrow screen shows
 * the one view its bottom navigation chose. Nothing hidden is ever a target, so
 * the key cannot move focus, or a pane's active state, somewhere unseen.
 *
 * A keyboard that is nowhere on the ring enters it at the near end: the first
 * spot going forward, the last going back. That is where the TUI would be too,
 * since its keyboard is never nowhere.
 */
export function nextFocus(ring: FocusRing, delta: 1 | -1): FocusSpot | null {
  const spots = onScreen(ring);
  if (spots.length === 0) return null;
  const at = ring.at;
  const from = at === null ? -1 : spots.findIndex((spot) => same(spot, at));
  let to: FocusSpot;
  if (from === -1) {
    to = delta > 0 ? spots[0] : spots[spots.length - 1];
  } else {
    to = spots[(from + delta + spots.length) % spots.length];
  }
  // A ring of one is a spot with nothing to move to. Focusing it again would be
  // harmless and would also announce a move that did not happen.
  return at !== null && same(to, at) ? null : to;
}

function onScreen({ paneCount, maximized, narrow, mobileView }: FocusRing): FocusSpot[] {
  const list: FocusSpot = { kind: "list" };
  const content: FocusSpot = { kind: "content" };
  const panes: FocusSpot[] = Array.from({ length: paneCount }, (_, index) => ({
    kind: "pane",
    index,
  }));
  if (narrow) {
    switch (mobileView) {
      case "files":
        return [list];
      case "diff":
        return [content];
      case "terminal":
        return panes;
    }
  }
  switch (maximized) {
    case "terminal":
      return panes;
    case "files":
      return [list, content];
    case "none":
      return [list, content, ...panes];
  }
}

function same(a: FocusSpot, b: FocusSpot): boolean {
  if (a.kind !== b.kind) return false;
  return a.kind !== "pane" || b.kind !== "pane" || a.index === b.index;
}
