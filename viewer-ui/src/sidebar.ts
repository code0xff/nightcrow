// The file sidebar's width, adjustable by dragging the divider and shared
// across the viewer's clients the same way the accent is (see `theme.ts` and
// `prefs.rs`): stored server-side so a phone and a laptop open at the same
// split, with a localStorage copy for the instant before the bundle loads.

import { useCallback, useState } from "react";
import { api } from "./api";

const STORAGE_KEY = "nightcrow.sidebarWidth";

// The bounds mirror `MIN_SIDEBAR_WIDTH` / `MAX_SIDEBAR_WIDTH` /
// `DEFAULT_SIDEBAR_WIDTH` in `src/web/viewer/prefs.rs`; the server clamps to the
// same range, so a value from either side round-trips unchanged.
export const MIN_SIDEBAR_WIDTH = 280;
export const MAX_SIDEBAR_WIDTH = 720;
export const DEFAULT_SIDEBAR_WIDTH = 460;
/** The sidebar never takes more than this share of the window, so the diff pane
 *  stays usable on a small screen whatever absolute width was stored. This is
 *  the bound that actually bites on a phone; the fixed ceiling bounds a large
 *  monitor. The grid track applies the same cap in CSS (`min(px, N vw)`) so a
 *  window *resize* re-caps the display instantly, without waiting for a poll or
 *  a drag to re-run the clamp below. */
export const MAX_SIDEBAR_VIEWPORT_FRACTION = 0.5;

/** Clamp a width to the fixed absolute bounds only. This is the *stored*
 *  preference: it is viewport-independent, so a wide width kept by another
 *  device (or set before the window shrank) survives, and the CSS grid track —
 *  `min(px, 50vw)` — is what caps the *display* on a narrow window. Capping the
 *  stored value here instead would lose the preference the moment it is read on
 *  a small screen, and never bring it back when the window widened. */
export function clampSidebarWidth(px: number): number {
  // Round to a whole pixel: pointer coordinates are fractional under browser
  // zoom / high-DPI, and the server field is a `u32` — a fractional value would
  // 400 the POST, so the width would apply locally and then revert on polling.
  return Math.min(Math.max(Math.round(px), MIN_SIDEBAR_WIDTH), MAX_SIDEBAR_WIDTH);
}

/** Clamp a width the user is dragging to, so the divider cannot be dragged past
 *  the viewport share the display would cap anyway — otherwise it would lag the
 *  pointer once the pane hit 50vw. The minimum wins if the share is below it on
 *  a very narrow window, so the status letters never clip. */
export function clampSidebarDragWidth(px: number): number {
  let ceiling = MAX_SIDEBAR_WIDTH;
  try {
    ceiling = Math.min(
      ceiling,
      Math.round(window.innerWidth * MAX_SIDEBAR_VIEWPORT_FRACTION),
    );
  } catch {
    // No window (tests/SSR): the fixed ceiling still bounds it.
  }
  // Rounded for the same reason as `clampSidebarWidth`: the committed value is a
  // whole-pixel `u32` on the wire.
  return Math.min(
    Math.max(Math.round(px), MIN_SIDEBAR_WIDTH),
    Math.max(ceiling, MIN_SIDEBAR_WIDTH),
  );
}

function loadWidth(): number {
  try {
    const raw = Number(localStorage.getItem(STORAGE_KEY));
    return Number.isFinite(raw) && raw > 0
      ? clampSidebarWidth(raw)
      : DEFAULT_SIDEBAR_WIDTH;
  } catch {
    return DEFAULT_SIDEBAR_WIDTH;
  }
}

function storeWidth(px: number) {
  try {
    localStorage.setItem(STORAGE_KEY, String(px));
  } catch {
    // Lost on reload; the session still renders correctly.
  }
}

/**
 * Current sidebar width plus the two ways it changes, mirroring `useAccent`.
 *
 * `resize` runs continuously while the divider is dragged — local only, so it
 * tracks the pointer without a request per pixel. `commit` runs once on release
 * and writes through to the server. `adopt` takes the server's value back on a
 * poll without echoing it. As with the accent, the caller owns the ordering
 * between `commit` and `adopt` so a poll already in flight when the user
 * finished dragging does not undo the drag (see `App.tsx`).
 *
 * A failed write leaves the width applied locally — the drag must not look like
 * it did nothing — and the next poll then corrects it.
 */
export function useSidebarWidth() {
  const [width, setWidth] = useState(loadWidth);

  // Drag input caps to the viewport share so the divider tracks the pointer;
  // adopt/load keep the absolute value so a wide preference is not lost when
  // read on a narrow screen.
  const resize = useCallback((px: number) => {
    const clamped = clampSidebarDragWidth(px);
    setWidth(clamped);
    storeWidth(clamped);
  }, []);

  const commit = useCallback((px: number) => {
    const clamped = clampSidebarDragWidth(px);
    setWidth(clamped);
    storeWidth(clamped);
    void api.setSidebarWidth(clamped).catch(() => {
      // Kept locally for this session; the next poll re-reads the server.
    });
  }, []);

  // Reset to the default. Uses the absolute clamp, not the drag clamp, so a
  // reset on a narrow window stores the real default rather than the
  // viewport-capped value the divider could be dragged to there.
  const reset = useCallback(() => {
    const clamped = clampSidebarWidth(DEFAULT_SIDEBAR_WIDTH);
    setWidth(clamped);
    storeWidth(clamped);
    void api.setSidebarWidth(clamped).catch(() => {
      // Kept locally for this session; the next poll re-reads the server.
    });
  }, []);

  /** Apply the server's value without writing it back. */
  const adopt = useCallback((remote: number) => {
    setWidth((current) => {
      const clamped = clampSidebarWidth(remote);
      if (clamped === current) return current;
      storeWidth(clamped);
      return clamped;
    });
  }, []);

  return { width, resize, commit, reset, adopt };
}
