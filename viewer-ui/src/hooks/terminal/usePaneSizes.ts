import { useCallback, useEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import type { PaneViewMode } from "../../lib/terminal/paneViewMode";
import type { ScreenScale } from "../ui/screenScale";
import {
  sendTerminalMessage,
  type PaneSize,
} from "../../api/terminal";

/** How long the layout must hold still before the PTY is told its new size.
 *
 *  A resize is not a cheap message: the child gets SIGWINCH and a full-screen
 *  program answers it by repainting from scratch. The browser reaches its
 *  final geometry through several intermediate ones, and forwarding each one
 *  makes the child redraw once per step. Waiting for the layout to settle
 *  sends the one size the user actually ended up with, while staying
 *  imperceptible during a divider drag. */
const SETTLE_MS = 60;

interface UsePaneSizesArgs {
  panes: number[];
  size: { w: number; h: number };
  zoomed: number | null;
  /** How the panel arranges its panes, read here as a relayout signal. The
   *  arrangement decides every pane's box — a tab is the whole panel and carries
   *  no header of its own, a grid cell is a fraction of it minus that header —
   *  while the container holding them keeps its pixels either way. So switching
   *  moves nothing else this hook watches, and without it the panes would keep
   *  the size the other arrangement gave them. */
  mode: PaneViewMode;
  screenScale: ScreenScale;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  /** Each pane's grid as the server has confirmed it, from `created` and
   *  `resized`. Read here for the panes this page is not sizing. */
  ptySizesRef: MutableRefObject<Map<number, PaneSize>>;
  /** The latest local fit, which may differ from xterm after a stale resize ack. */
  desiredSizesRef: MutableRefObject<Map<number, PaneSize>>;
  /** What this page has already asked for, so an unchanged layout is not sent
   *  twice. Separate from the confirmed sizes because a request is not an
   *  outcome: the server drops one from a page that lost the sizing between
   *  laying the frame out and the message arriving, and recording that as the
   *  pane's size would render every client's view of it at a grid the child
   *  never had. */
  askedSizesRef: MutableRefObject<Map<number, PaneSize>>;
  /** Whether this page's layout is what sets the pane sizes. When it is not,
   *  the panes are at another client's size and this page renders that grid
   *  instead of fitting its own — a PTY has one size, and the child cannot be
   *  re-flowed after the fact. */
  ownsSize: boolean;
  /** Whether the layout is still resolving and nothing should be fitted to it
   *  yet. True while a replayed zoom is waiting for the pane it names: the
   *  panel will end up filled by that one pane, so anything fitted to a grid
   *  cell meanwhile is measured against a layout that is about to be replaced —
   *  and, being hidden by then, keeps the size it was wrongly given. */
  layoutPending: boolean;
  /** Whether a soft keyboard is covering the lower part of the page. The panel
   *  is shorter while it is, but the PTY is not told: a resize is a SIGWINCH
   *  and a full repaint, and a keyboard that comes and goes with every message
   *  typed would have the child repaint from the top each time — twice. The
   *  panes keep the grid they had and the cell shows the bottom of it
   *  (`TerminalCell`), which is where the prompt is. */
  keyboardOpen: boolean;
  /** Incremented by a manual fit command, even while this page owns sizing. */
  refitEpoch: number;
}

/**
 * Give each visible pane the size it should have, and — when this page is the
 * one deciding that — tell the server once the layout stops moving.
 *
 * The local fit runs immediately: it only reflows xterm's own buffer, costs
 * nothing on the wire, and keeping it in step with the DOM is what makes a drag
 * look right. Only the outbound resize is held back until the geometry settles.
 *
 * A page that does not own the sizing fits nothing. It sets each pane to the
 * size the session reports and lets the cell crop or pad the result, because
 * the alternative is an emulator wrapping somewhere the child does not.
 */
export function usePaneSizes({
  panes,
  size,
  zoomed,
  mode,
  screenScale,
  socketRef,
  viewsRef,
  bodyRefs,
  ptySizesRef,
  desiredSizesRef,
  askedSizesRef,
  ownsSize,
  layoutPending,
  keyboardOpen,
  refitEpoch,
}: UsePaneSizesArgs) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const seenRefitEpoch = useRef(refitEpoch);

  const flush = useCallback(() => {
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      const desired = desiredSizesRef.current.get(pane) ?? {
        rows: view.term.rows,
        cols: view.term.cols,
      };
      const asked = askedSizesRef.current.get(pane);
      if (
        asked &&
        asked.rows === desired.rows &&
        asked.cols === desired.cols
      ) {
        continue;
      }
      if (
        sendTerminalMessage(socketRef.current, {
          type: "resize",
          pane,
          rows: desired.rows,
          cols: desired.cols,
        })
      ) {
        askedSizesRef.current.set(pane, desired);
      }
    }
  }, [socketRef, viewsRef, bodyRefs, askedSizesRef, desiredSizesRef]);

  // Size visible panes after layout changes; never resize hidden cells to zero.
  useEffect(() => {
    // A keyboard-shortened panel is not a layout the panes should take. Nothing
    // is fitted and nothing goes out; the effect runs again when it closes and
    // finds the panes already at the size the restored panel gives them.
    if (keyboardOpen) return;
    if (seenRefitEpoch.current !== refitEpoch) {
      askedSizesRef.current.clear();
      seenRefitEpoch.current = refitEpoch;
    }
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      // Take the size the PTY actually has whenever this page's cells are not
      // the answer: it is a spectator, or its layout has not resolved yet.
      // Not merely "skip the fit" — an emulator at any other size renders
      // this pane wrapping where the child does not. A pane opens at the
      // PTY's size (`useTerminalViews`) and `resized` follows it from then
      // on, so this covers the rest: a layout change here, and a page that
      // has just lost the sizing while its panes are at its own fit.
      if (!ownsSize || layoutPending) {
        const pty = ptySizesRef.current.get(pane);
        if (pty) {
          view.term.resize(pty.cols, pty.rows);
          desiredSizesRef.current.set(pane, pty);
        }
        continue;
      }
      view.fit.fit();
      desiredSizesRef.current.set(pane, {
        rows: view.term.rows,
        cols: view.term.cols,
      });
    }
    // Nothing goes out while the layout is still resolving: what would be
    // measured is a grid about to be replaced.
    if (!ownsSize || layoutPending) return;
    // Each layout change restarts the wait, so a run of them costs one resize
    // rather than one apiece.
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(flush, SETTLE_MS);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [
    panes,
    zoomed,
    mode,
    screenScale,
    refitEpoch,
    size,
    flush,
    viewsRef,
    bodyRefs,
    ptySizesRef,
    desiredSizesRef,
    ownsSize,
    layoutPending,
    keyboardOpen,
  ]);
}
