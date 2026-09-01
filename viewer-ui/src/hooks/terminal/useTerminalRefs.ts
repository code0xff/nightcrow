import { useRef } from "react";
import type { PaneView } from "../../lib/terminalLayout";
import type { PaneSize } from "../../api/terminal";

/**
 * The mutable state the terminal panel's hooks share.
 *
 * Grouped because they are one thing: every hook the panel mounts is handed some
 * subset of these, and they are refs rather than state because what reads them
 * is a socket callback, a resize observer or an xterm — none of which are allowed
 * to wait for a render, and none of which should cause one.
 */
export function useTerminalRefs() {
  const containerRef = useRef<HTMLDivElement>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const viewsRef = useRef(new Map<number, PaneView>());
  const bodyRefs = useRef(new Map<number, HTMLDivElement>());
  // Each pane's grid as the server has confirmed it — `created` and `resized`,
  // never a size this page has merely asked for. What a pane is rendered at is
  // read from here, and the server drops a resize from a page that lost the
  // sizing mid-flight, so a request must not be recorded as fact.
  const ptySizesRef = useRef(new Map<number, PaneSize>());
  // What this page last asked each pane's size to be, so an unchanged layout
  // does not send the same resize again.
  const askedSizesRef = useRef(new Map<number, PaneSize>());
  // Buffer scrollback received before the corresponding xterm exists.
  const pendingRef = useRef(new Map<number, Uint8Array[]>());
  // A zoom this page has asked for and not yet been answered. Held here because
  // both halves need it: the commands read it, the socket clears it.
  const zoomAskedRef = useRef<number | null | undefined>(undefined);
  // Cells rendered for startup terminals the server has not created yet, so
  // their size can be measured from the slot each will occupy.
  const slotRefs = useRef(new Map<number, HTMLDivElement>());
  return {
    containerRef,
    socketRef,
    viewsRef,
    bodyRefs,
    ptySizesRef,
    askedSizesRef,
    pendingRef,
    zoomAskedRef,
    slotRefs,
  };
}

/** The bag above, passed as a unit wherever a hook needs several of them. */
export type TerminalRefs = ReturnType<typeof useTerminalRefs>;
