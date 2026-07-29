import { useCallback, useEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import type { PaneView } from "../../lib/terminalLayout";

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
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  /** Sizes the PTYs are already known to have, so an unchanged size is never
   *  sent. Seeded from `created`, updated here and by `resized`. */
  sentSizesRef: MutableRefObject<Map<number, { rows: number; cols: number }>>;
  /** Whether this page's layout is what sets the pane sizes. When it is not,
   *  the panes are at another client's size and this page renders that grid
   *  instead of fitting its own — a PTY has one size, and the child cannot be
   *  re-flowed after the fact. */
  ownsSize: boolean;
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
  socketRef,
  viewsRef,
  bodyRefs,
  sentSizesRef,
  ownsSize,
}: UsePaneSizesArgs) {
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flush = useCallback(() => {
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      const { rows, cols } = view.term;
      const sent = sentSizesRef.current.get(pane);
      if (sent && sent.rows === rows && sent.cols === cols) continue;
      sentSizesRef.current.set(pane, { rows, cols });
      socketRef.current?.send(
        JSON.stringify({ type: "resize", pane, rows, cols }),
      );
    }
  }, [socketRef, viewsRef, bodyRefs, sentSizesRef]);

  // Size visible panes after layout changes; never resize hidden cells to zero.
  useEffect(() => {
    for (const [pane, view] of viewsRef.current) {
      const body = bodyRefs.current.get(pane);
      if (!body || body.clientHeight === 0 || body.clientWidth === 0) continue;
      if (ownsSize) {
        view.fit.fit();
        continue;
      }
      // A pane created while this page was a spectator starts at xterm's
      // default until this runs; `resized` covers it from then on.
      const pty = sentSizesRef.current.get(pane);
      if (pty) view.term.resize(pty.cols, pty.rows);
    }
    if (!ownsSize) return;
    // Each layout change restarts the wait, so a run of them costs one resize
    // rather than one apiece.
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(flush, SETTLE_MS);
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [panes, zoomed, size, flush, viewsRef, bodyRefs, sentSizesRef, ownsSize]);
}
