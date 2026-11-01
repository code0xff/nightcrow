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

/** Whether this width offers a choice at all. */
export function canChoosePaneViewMode(viewportWidth: number): boolean {
  return viewportWidth >= GRID_MIN_VIEWPORT_PX;
}

/**
 * The arrangement to draw: tabs below `md` whatever was stored, the stored
 * choice above it, the default where nothing was stored.
 *
 * Below `md` this is not a preference to respect but a layout that does not
 * work. A split grid hands each pane fewer columns than a command line needs,
 * and a PTY has one size for the whole session — a phone that took the sizing
 * in a grid would narrow every other screen's panes along with its own. The
 * stored choice is kept, not cleared, so widening the window gives it back.
 */
export function resolvePaneViewMode(
  stored: PaneViewMode | null,
  viewportWidth: number,
): PaneViewMode {
  if (!canChoosePaneViewMode(viewportWidth)) return "tabs";
  return stored ?? defaultPaneViewMode(viewportWidth);
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
 * Every pane is stacked at the panel's full size and keeps that box whether or
 * not it is the tab on screen, so switching tabs needs no resize, SIGWINCH or
 * repaint.
 *
 * A hidden pane is moved out of the stack's box rather than merely made
 * invisible. xterm pauses its renderer from an IntersectionObserver, and an
 * element at `inset: 0` still intersects even when `visibility: hidden` — so a
 * background pane would keep painting every frame it receives. A transform
 * moves the cell without changing its size, so its box (and therefore its
 * fitted grid) is kept. The stack container has to clip it for the move to
 * count as leaving the viewport; see `STACKED_STACK_CLASS`.
 */
export function stackedCellStyle(shown: boolean): CSSProperties {
  return {
    position: "absolute",
    inset: 0,
    display: "flex",
    visibility: shown ? "visible" : "hidden",
    // A hair past its own width, so the moved cell shares no edge with the
    // stack: an intersection of zero area is not an intersection, but relying
    // on that boundary would be needlessly exact.
    transform: shown ? undefined : "translateX(calc(-100% - 2px))",
  };
}

/**
 * The container tabs are stacked in. It clips, which is what makes
 * `stackedCellStyle`'s out-of-view transform hide the cell from
 * IntersectionObserver instead of leaving it overlapping the page.
 */
export const STACKED_STACK_CLASS = "relative h-full overflow-hidden";
