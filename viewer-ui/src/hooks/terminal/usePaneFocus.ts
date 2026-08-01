import { useEffect } from "react";
import type { MutableRefObject } from "react";
import type { PaneView } from "../../lib/terminalLayout";
import { zoomPending } from "../../lib/zoom";

interface UsePaneFocusArgs {
  repo: string;
  panes: number[];
  active: number | null;
  setActive: React.Dispatch<React.SetStateAction<number | null>>;
  /** What the server says fills the panel, before it is checked against the
   *  panes here — the raw value, because "a zoom is coming" is a state only it
   *  can express. */
  zoomed: number | null;
  /** The zoom actually being rendered, or null for the grid. */
  zoom: number | null;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  /** The pane each repository was last typed into, so returning to one comes
   *  back to it. Written here as well as by a click, because a zoom moves the
   *  keyboard too. */
  lastActiveByRepoRef: MutableRefObject<Map<string, number>>;
}

/**
 * Which pane has the keyboard.
 *
 * Three rules that have to agree, which is why they are in one place: something
 * is focused once there are panes; the pane filling the panel is the one being
 * typed into; and whatever ends up active gets the actual DOM focus.
 *
 * The middle rule is the one that is easy to miss. A zoom no longer needs a
 * click on this page to happen — it is replayed on connect and set by other
 * clients — so nothing else would move the keyboard off a pane that is no
 * longer on screen. The key bar types into the active pane as well, so this
 * decides where an on-screen Escape goes, not only a keystroke.
 */
export function usePaneFocus({
  repo,
  panes,
  active,
  setActive,
  zoomed,
  zoom,
  viewsRef,
  lastActiveByRepoRef,
}: UsePaneFocusArgs) {
  useEffect(() => {
    // Not while a replay has named a zoom whose pane has not arrived: the panes
    // come one at a time, and focusing an earlier one would put the keyboard in
    // a terminal that is about to be replaced by the zoomed one.
    if (zoomPending(zoomed, panes)) return;
    if (active === null && panes.length > 0) {
      const remembered = lastActiveByRepoRef.current.get(repo);
      setActive(
        remembered !== undefined && panes.includes(remembered)
          ? remembered
          : panes[panes.length - 1],
      );
    }
  }, [active, panes, repo, zoomed, setActive, lastActiveByRepoRef]);

  useEffect(() => {
    if (zoom !== null && zoom !== active) {
      setActive(zoom);
      lastActiveByRepoRef.current.set(repo, zoom);
    }
  }, [zoom, active, repo, setActive, lastActiveByRepoRef]);

  useEffect(() => {
    if (active !== null) viewsRef.current.get(active)?.term.focus();
  }, [active, viewsRef]);
}
