import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { PaneView } from "../../lib/terminalLayout";

const TERM_FONT_SIZE =
  typeof window !== "undefined" &&
  typeof window.matchMedia === "function" &&
  window.matchMedia("(pointer: coarse)").matches
    ? 13
    : 12;

interface UseTerminalViewsArgs {
  panes: number[];
  // Reveal signals: opening xterm inside a hidden (display:none) cell caches a
  // 0x0 character-cell measurement that fit() can never recover, so creation
  // waits until the cell has size. Re-run on the layout changes that can reveal
  // a cell — the container resizing (`size`) or a zoom toggle (`zoomed`) — which
  // mirrors the fit effect's own dependency set.
  size: { w: number; h: number };
  zoomed: number | null;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  pendingRef: MutableRefObject<Map<number, Uint8Array[]>>;
  setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>>;
}

export function useTerminalViews({
  panes,
  size,
  zoomed,
  socketRef,
  viewsRef,
  bodyRefs,
  pendingRef,
  setTitles,
}: UseTerminalViewsArgs) {
  useEffect(() => {
    for (const pane of panes) {
      if (viewsRef.current.has(pane)) continue;
      const body = bodyRefs.current.get(pane);
      if (!body) continue;
      // Defer creation until the cell is visible; retried on the reveal that
      // changes `size`. Buffered output is held in pendingRef until then.
      if (body.clientHeight === 0 || body.clientWidth === 0) continue;

      const term = new Terminal({
        fontFamily: getComputedStyle(document.body).fontFamily,
        fontSize: TERM_FONT_SIZE,
        theme: { background: "#0b0b0d", foreground: "#e6e6ec" },
        cursorBlink: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.onData((data) =>
        socketRef.current?.send(JSON.stringify({ type: "input", pane, data })),
      );
      // Preserve the previous label when OSC provides an empty title.
      term.onTitleChange((title) => {
        const cleaned = title.replace(/\s+/g, " ").trim();
        if (!cleaned) return;
        setTitles((current) => ({ ...current, [pane]: cleaned }));
      });
      term.open(body);
      viewsRef.current.set(pane, { term, fit });

      const queued = pendingRef.current.get(pane);
      if (queued) {
        for (const chunk of queued) term.write(chunk);
        pendingRef.current.delete(pane);
      }
    }

    for (const [pane, view] of viewsRef.current) {
      if (!panes.includes(pane)) {
        view.term.dispose();
        viewsRef.current.delete(pane);
      }
    }
  }, [panes, size, zoomed]);
}
