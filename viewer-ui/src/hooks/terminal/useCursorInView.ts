import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { cursorWindowShift } from "../../lib/terminal/cursorWindow";
import type { PaneView } from "../../lib/terminal/terminalLayout";

interface UseCursorInViewArgs {
  panes: number[];
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  /** Re-read as a signal: opening and closing it is what crops the panes. */
  keyboardOpen: boolean;
  /** The panel's box, the signal `useTerminalViews` opens a deferred pane on —
   *  without it a pane opened by a later reveal would never be watched. */
  size: { w: number; h: number };
}

/**
 * Keep the cursor's row in view when a pane is cropped — see
 * `lib/terminal/cursorWindow.ts` for why the plain bottom crop is not enough.
 *
 * The cell already anchors the terminal to its bottom (`justify-end`); this
 * only adjusts how far below that edge the terminal sits, so the crop stays a
 * layout offset like the one it refines, and xterm's pointer and selection
 * geometry, read from its element's box, follow it. Nothing is resized.
 */
export function useCursorInView({
  panes,
  viewsRef,
  bodyRefs,
  keyboardOpen,
  size,
}: UseCursorInViewArgs) {
  useEffect(() => {
    const cleanups: (() => void)[] = [];
    for (const pane of panes) {
      const view = viewsRef.current.get(pane);
      const body = bodyRefs.current.get(pane);
      const element = view?.term.element;
      if (!view || !body || !element) continue;
      const { term } = view;

      const align = () => {
        const buffer = term.buffer.active;
        const shift = cursorWindowShift({
          termHeight: element.offsetHeight,
          bodyHeight: body.clientHeight,
          rows: term.rows,
          cursorRow: buffer.cursorY,
          atLive: buffer.viewportY === buffer.baseY,
        });
        const value = shift > 0 ? `-${shift}px` : "";
        if (element.style.marginBottom !== value) element.style.marginBottom = value;
      };

      align();
      const render = term.onRender(align);
      const scroll = term.onScroll(align);
      const observer = new ResizeObserver(align);
      observer.observe(body);
      cleanups.push(() => {
        render.dispose();
        scroll.dispose();
        observer.disconnect();
      });
    }
    return () => {
      for (const cleanup of cleanups) cleanup();
    };
  }, [panes, viewsRef, bodyRefs, keyboardOpen, size]);
}
