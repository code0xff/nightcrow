import { useEffect, useRef } from "react";

/** Return true to consume the key; false leaves the event untouched. */
export type GlobalKeydownHandler = (event: KeyboardEvent) => boolean;

/**
 * One capture-phase keydown listener on `document`, shared by every page-level
 * shortcut.
 *
 * Why capture on `document` and not a React `onKeyDown`: xterm reads keys from
 * a `keydown` listener on its own hidden `<textarea>` (see
 * `hooks/terminal/useTerminalViews.ts`, `attachCustomKeyEventHandler`), and
 * that textarea is a descendant of `document`. A capture-phase listener on
 * `document` therefore runs strictly before it, so calling
 * `stopImmediatePropagation` there means xterm never sees the key at all —
 * `onData` never fires and not one byte reaches the PTY. Bubble-phase or
 * component-level handling cannot make that promise: xterm has already
 * encoded and sent the key by the time the event gets there.
 *
 * All three calls are needed to consume: `preventDefault` for the browser's
 * own gesture, `stopPropagation` for later listeners up the tree, and
 * `stopImmediatePropagation` for other listeners on `document` itself.
 *
 * Returning false must leave the event completely alone — no `preventDefault` —
 * so an unclaimed chord still reaches the pane as its normal escape sequence.
 */
export function useGlobalKeydown(
  handler: GlobalKeydownHandler,
  enabled = true,
): void {
  // Held in a ref so a handler that closes over changing state does not
  // re-subscribe: the listener would then be removed and re-added on every
  // render, and a key pressed in that window has no listener to answer it.
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (!handlerRef.current(event)) return;
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
    };
    document.addEventListener("keydown", onKeyDown, { capture: true });
    return () =>
      document.removeEventListener("keydown", onKeyDown, { capture: true });
  }, [enabled]);
}
