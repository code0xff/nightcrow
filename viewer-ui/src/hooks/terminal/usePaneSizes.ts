import { useCallback, useEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import type { PaneView } from "../../lib/terminalLayout";
import type { PaneViewMode } from "../../lib/paneViewMode";
import {
  sendTerminalMessage,
  type PaneSize,
} from "../../api/terminal";

/** How long the layout must hold still before the PTY is told its new size.
 *
 *  A resize is not a cheap message: the child gets SIGWINCH and a full-screen
 *  program answers it by repainting from scratch. The browser reaches its
 *  final geometry through several intermediate ones — a web font finishing,
 *  the grid re-splitting as another pane appears, a window animating, a
 *  breakpoint flipping — and forwarding each one makes the child redraw once
 *  per step. Waiting for the layout to settle sends the one size the user
 *  actually ended up with. Short enough to stay imperceptible while dragging
 *  a divider. */
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
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  /** Sizes the PTYs are already known to have, so an unchanged size is never
   *  sent. Seeded from `created`, updated here and by `resized`. */
  sentSizesRef: MutableRefObject<Map<number, PaneSize>>;
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
  socketRef,
  viewsRef,
  bodyRefs,
  sentSizesRef,
  ownsSize,
  layoutPending,
}: UsePaneSizesArgs) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flush = useCallback(() => {
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      const { rows, cols } = view.term;
      const sent = sentSizesRef.current.get(pane);
      if (sent && sent.rows === rows && sent.cols === cols) continue;
      if (
        sendTerminalMessage(socketRef.current, {
          type: "resize",
          pane,
          rows,
          cols,
        })
      ) {
        sentSizesRef.current.set(pane, { rows, cols });
      }
    }
  }, [socketRef, viewsRef, bodyRefs, sentSizesRef]);

  // Size visible panes after layout changes; never resize hidden cells to zero.
  useEffect(() => {
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      // Take the size the PTY actually has whenever this page's cells are not
      // the answer: it is a spectator, or its layout has not resolved yet. Not
      // merely "skip the fit" — an emulator left at its 80×24 default parses
      // the replay that is arriving right now at the wrong width, and what was
      // drawn outside it is gone. A pane created while this page was a
      // spectator starts at that default until this runs; `resized` covers it
      // from then on.
      if (!ownsSize || layoutPending) {
        const pty = sentSizesRef.current.get(pane);
        if (pty) view.term.resize(pty.cols, pty.rows);
        continue;
      }
      view.fit.fit();
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
    size,
    flush,
    viewsRef,
    bodyRefs,
    sentSizesRef,
    ownsSize,
    layoutPending,
  ]);
}
