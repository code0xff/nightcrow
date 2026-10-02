/// How the terminal panel arranges the panes it is given: the split grid the TUI
/// draws, or one pane at a time behind a tab strip. A rendering choice only —
/// the session's pane list, order and zoom are the server's either way.

import type { CSSProperties } from "react";

export type PaneViewMode = "grid" | "tabs";

/** Tailwind's `md` breakpoint. Below it a split grid gives each pane fewer
 *  columns than a command line needs, so tabs are the default there. */
export const GRID_MIN_VIEWPORT_PX = 768;

export function defaultPaneViewMode(viewportWidth: number): PaneViewMode {
  return viewportWidth < GRID_MIN_VIEWPORT_PX ? "tabs" : "grid";
}

/** What was stored, or null for anything this version does not recognise. */
export function parsePaneViewMode(raw: string | null): PaneViewMode | null {
  return raw === "grid" || raw === "tabs" ? raw : null;
}

/**
 * Which pane the tab strip shows.
 *
 * Derived from the pane list rather than trusted, for the reason `renderedZoom`
 * is: the focus and the panes arrive as separate frames, so `active` can name a
 * pane that has just exited or has not been replayed yet. Falling back to the
 * first pane keeps the panel showing a terminal through that gap instead of
 * going blank — there is no grid behind the tabs to fall back to.
 */
export function shownTab(active: number | null, panes: number[]): number | null {
  if (active !== null && panes.includes(active)) return active;
  return panes[0] ?? null;
}

/**
 * The cell a pane gets in tabs mode.
 *
 * Every pane is stacked at the panel's full size and the hidden ones are merely
 * invisible, not `display: none`. A pane with no layout box measures zero, and
 * zero is what makes `useTerminalViews` defer opening it and `usePaneSizes` skip
 * it — so laying the tabs out would cost a fit, a SIGWINCH and a full repaint on
 * every switch. Stacked, all tabs already hold the size they will be shown at
 * and switching moves no bytes.
 */
export function stackedCellStyle(shown: boolean): CSSProperties {
  return {
    position: "absolute",
    inset: 0,
    display: "flex",
    visibility: shown ? "visible" : "hidden",
  };
}
