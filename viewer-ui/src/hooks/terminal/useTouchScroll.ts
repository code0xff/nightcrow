import { useCallback, useRef } from "react";
import type { MutableRefObject } from "react";
import type { PaneView } from "../../lib/terminalLayout";
import {
  advanceTouchScroll,
  beginTouchScroll,
  type TouchScroll,
} from "../../lib/touchScroll";

interface UseTouchScrollArgs {
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  /** The element each pane's terminal is opened into, which is also what the
   *  drag handlers are attached to — so this is how a gesture finds its pane. */
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
}

/**
 * Scrolling a terminal by dragging it, for the screens that have no wheel.
 *
 * The drag turns a wheel and lets xterm route it, so a program that reads the
 * wheel itself — an agent, a pager — gets the mouse report it is waiting for
 * without this having to work out which programs those are. What xterm does not
 * claim is scrolled here instead: it leaves the scrollback to the browser's own
 * scrolling, and a synthetic event never triggers that. `defaultPrevented` is
 * the seam — xterm cancels a wheel it turned into input and leaves one it did
 * not.
 *
 * One pointer at a time. A second finger is a pinch, not a scroll, and taking
 * either as travel would fight the gesture the person is making.
 */
export function useTouchScroll({ viewsRef, bodyRefs }: UseTouchScrollArgs) {
  const pointerRef = useRef<number | null>(null);
  const stateRef = useRef<TouchScroll | null>(null);

  const end = useCallback(() => {
    pointerRef.current = null;
    stateRef.current = null;
  }, []);

  const onPointerDown = useCallback(
    (event: React.PointerEvent) => {
      if (event.pointerType !== "touch") return;
      if (pointerRef.current !== null) {
        end();
        return;
      }
      pointerRef.current = event.pointerId;
      stateRef.current = beginTouchScroll(event.clientY);
    },
    [end],
  );

  const onPointerMove = useCallback(
    (event: React.PointerEvent) => {
      if (event.pointerId !== pointerRef.current) return;
      const state = stateRef.current;
      if (!state) return;
      const { next, deltaY } = advanceTouchScroll(state, event.clientY);
      stateRef.current = next;
      if (deltaY === 0) return;
      // Only once this is a scroll, so a tap still reaches the terminal.
      event.preventDefault();
      const body = event.currentTarget as HTMLElement;
      scrollPane(body, event, deltaY, paneViewOf(body, viewsRef, bodyRefs));
    },
    [viewsRef, bodyRefs],
  );

  const onPointerUp = useCallback(
    (event: React.PointerEvent) => {
      if (event.pointerId === pointerRef.current) end();
    },
    [end],
  );

  return {
    onPointerDown,
    onPointerMove,
    onPointerUp,
    onPointerCancel: onPointerUp,
  };
}

function paneViewOf(
  body: HTMLElement,
  viewsRef: MutableRefObject<Map<number, PaneView>>,
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>,
): PaneView | undefined {
  for (const [pane, node] of bodyRefs.current) {
    if (node === body) return viewsRef.current.get(pane);
  }
  return undefined;
}

function scrollPane(
  body: HTMLElement,
  at: { clientX: number; clientY: number },
  deltaY: number,
  view: PaneView | undefined,
) {
  const element = body.querySelector<HTMLElement>(".xterm");
  if (!element) return;
  const wheel = new WheelEvent("wheel", {
    deltaY,
    deltaMode: WheelEvent.DOM_DELTA_PIXEL,
    // The pane's program is told where the wheel turned, so the coordinates have
    // to be the finger's rather than the element's.
    clientX: at.clientX,
    clientY: at.clientY,
    bubbles: true,
    cancelable: true,
    view: window,
  });
  element.dispatchEvent(wheel);
  if (wheel.defaultPrevented || !view) return;

  // Nothing claimed the wheel, so this is the scrollback. Measured off the
  // terminal itself rather than assumed: the font follows the device, and a
  // notch has to be the same distance in rows that it was in pixels. Off the
  // terminal and not the body, because the two differ while a soft keyboard
  // has shortened the body around an unrefitted terminal.
  const rows = view.term.rows;
  const cellHeight = rows > 0 ? element.clientHeight / rows : 0;
  if (cellHeight <= 0) return;
  const lines = Math.round(deltaY / cellHeight);
  // At least a row per notch, so a slow drag moves rather than rounding away.
  view.term.scrollLines(deltaY < 0 ? Math.min(-1, lines) : Math.max(1, lines));
}
