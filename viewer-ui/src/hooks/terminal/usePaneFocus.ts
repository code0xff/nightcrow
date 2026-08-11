import { useEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import type { PaneView } from "../../lib/terminalLayout";
import type { PaneViewMode } from "../../lib/paneViewMode";
import { lastPaneOf, rememberPane } from "../../lib/lastPane";
import {
  focusHolder,
  focusIsTakeable,
  focusOnAttach,
  focusStep,
} from "../../lib/paneFocus";
import { zoomPending } from "../../lib/zoom";

interface UsePaneFocusArgs {
  repo: string;
  panes: number[];
  /** Panes the replay has promised and not yet delivered. */
  replayLeft: number;
  active: number | null;
  setActive: React.Dispatch<React.SetStateAction<number | null>>;
  /** What the server says fills the panel, before it is checked against the
   *  panes here — the raw value, because "a zoom is coming" is a state only it
   *  can express. */
  zoomed: number | null;
  /** The zoom actually being rendered, or null for the grid. */
  zoom: number | null;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  /** The element the panes are laid out in. Read to tell the panel's own
   *  elements from the rest of the page, and re-measured — with `mode` — as the
   *  signal that a cell has been revealed. */
  panelRef: React.RefObject<HTMLDivElement | null>;
  size: { w: number; h: number };
  mode: PaneViewMode;
}

/**
 * Which pane has the keyboard.
 *
 * Three rules that have to agree, which is why they are in one place: something
 * is focused once there are panes; the pane filling the panel is the one being
 * typed into; and the active pane keeps the actual DOM focus — not just when it
 * becomes active, but for as long as the layout lets it hold one.
 *
 * Which pane the first rule picks is whichever this screen last had the keyboard
 * on, kept in `lib/lastPane` — outside the page, because a reload is exactly
 * when the answer is needed and a reload is what used to lose it. The socket
 * restores it as its pane arrives; what is left to `focusOnAttach` here is the
 * screen that has no such pane. Either way the pane it settles on is written
 * back, so what is remembered always names a pane the session has.
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
  replayLeft,
  active,
  setActive,
  zoomed,
  zoom,
  viewsRef,
  panelRef,
  size,
  mode,
}: UsePaneFocusArgs) {
  useEffect(() => {
    // Not while a replay has named a zoom whose pane has not arrived: focusing
    // an earlier one would put the keyboard in a terminal that is about to be
    // replaced by the zoomed one. The rest of the waiting — for the panes
    // themselves — is `focusOnAttach`'s, and it is safe to wait on `replayLeft`
    // because the count is exact: `hello` says how many panes follow, and the
    // hub queues the whole replay under its lock before the client can be
    // broadcast to.
    if (zoomPending(zoomed, panes)) return;
    const chosen = focusOnAttach(active, panes, replayLeft, lastPaneOf(repo));
    if (chosen === null) return;
    setActive(chosen);
    // Written even though nobody chose it, because what is remembered has to
    // name a pane of the session on screen. Left pointing at a pane this
    // session does not have, it would be matched again the moment something
    // hands that id out — a new session starts numbering afresh — and the
    // keyboard would leave the pane this page is on for a terminal another
    // client just opened.
    rememberPane(repo, chosen);
  }, [active, panes, replayLeft, repo, zoomed, setActive]);

  useEffect(() => {
    if (zoom !== null && zoom !== active) {
      setActive(zoom);
      rememberPane(repo, zoom);
    }
  }, [zoom, active, repo, setActive]);

  // Where the keyboard actually is.
  //
  // Run on the same signals `useTerminalViews` opens panes on, not on `active`
  // alone, because two things other than a change of pane decide whether the
  // active one holds the keyboard:
  //
  //   - the xterm may not exist yet. Creation is deferred while the cell has no
  //     layout box, and panes arrive over the socket whether the panel is on
  //     screen or not, so `active` is routinely set before there is anything to
  //     focus.
  //   - hiding the panel takes the focus away. Below `md` the panel is
  //     `display: none` whenever another view is chosen, and an element that
  //     stops being rendered is blurred to the body.
  //
  // Either way `active` is unchanged when the panel comes back, so an effect
  // keyed on it alone leaves a panel that draws output, accepts the on-screen
  // key bar, and ignores the keyboard. This hook is declared after
  // `useTerminalViews`, so a pane opened by the same reveal is already here.
  //
  // What is asked on each of those signals is an edge — see `focusStep`. The
  // pane the panel holds is remembered rather than re-derived, because the DOM
  // cannot answer it: focus this page never had looks exactly like focus it had
  // and lost.
  const heldRef = useRef<number | null>(null);
  useEffect(() => {
    const view = active === null ? undefined : viewsRef.current.get(active);
    const element = view?.term.element;
    const step = focusStep(
      active,
      !!element && element.clientHeight > 0,
      heldRef.current,
    );
    if (!step.focus || !view) {
      heldRef.current = step.held;
      return;
    }
    // Left for a later signal rather than recorded as held: the panel wants the
    // keyboard and has not been given it.
    const holder = focusHolder(document.activeElement, panelRef.current);
    if (!focusIsTakeable(holder)) return;
    view.term.focus();
    heldRef.current = step.held;
  }, [active, panes, size, mode, zoom, viewsRef, panelRef]);
}
