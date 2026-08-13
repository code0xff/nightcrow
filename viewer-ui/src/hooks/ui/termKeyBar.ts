// Device-local, like the pane view mode: whether the keys this bar carries are
// reachable any other way is a fact about the device in front of you, not about
// the project, so it is not shared with the server.

import { useCallback, useEffect, useState } from "react";
import {
  KEYBOARD_MIN_VIEWPORT_PX,
  defaultKeyBarShown,
  parseKeyBarPref,
  type KeyBarPref,
} from "../../lib/termKeys";

const STORAGE_KEY = "nightcrow.termKeyBar";
const COARSE_QUERY = "(pointer: coarse)";

function load(): KeyBarPref | null {
  try {
    return parseKeyBarPref(localStorage.getItem(STORAGE_KEY));
  } catch {
    return null;
  }
}

function store(pref: KeyBarPref) {
  try {
    localStorage.setItem(STORAGE_KEY, pref);
  } catch {
    // A browser that refuses storage still gets the toggle, just not the memory.
  }
}

function coarseNow(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia(COARSE_QUERY).matches
  );
}

function viewportWidth(): number {
  return typeof window === "undefined"
    ? KEYBOARD_MIN_VIEWPORT_PX
    : window.innerWidth;
}

/**
 * Whether the terminal panel carries the on-screen key bar.
 *
 * The device decides until someone says otherwise, and then their choice sticks
 * — including across the rotation that would otherwise flip it back. Kept in one
 * place because the two halves have to agree: the toggle writes the override,
 * and the device is only consulted while there is none.
 */
export function useTermKeyBar() {
  const [override, setOverride] = useState(load);
  const [coarse, setCoarse] = useState(coarseNow);
  const [width, setWidth] = useState(viewportWidth);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const apply = () => setWidth(window.innerWidth);
    apply();
    window.addEventListener("resize", apply);
    return () => window.removeEventListener("resize", apply);
  }, []);

  // A pointer can change under a page that never reloads: an iPad picks up a
  // trackpad, a laptop folds into a tablet.
  useEffect(() => {
    if (typeof window === "undefined" || typeof window.matchMedia !== "function")
      return;
    const query = window.matchMedia(COARSE_QUERY);
    const apply = () => setCoarse(query.matches);
    apply();
    query.addEventListener("change", apply);
    return () => query.removeEventListener("change", apply);
  }, []);

  const shown = override
    ? override === "shown"
    : defaultKeyBarShown(coarse, width);

  const toggle = useCallback(() => {
    const next: KeyBarPref = shown ? "hidden" : "shown";
    store(next);
    setOverride(next);
  }, [shown]);

  return { shown, toggle };
}
