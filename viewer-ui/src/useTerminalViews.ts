import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { PaneView } from "./terminalLayout";

interface UseTerminalViewsArgs {
  panes: number[];
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  pendingRef: MutableRefObject<Map<number, Uint8Array[]>>;
  setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>>;
}

/// Materialise one xterm per pane, opened into that pane's cell body (rendered
/// keyed by pane so it survives grid reflows). `open()` runs once here; dispose
/// the views of panes that have gone away.
export function useTerminalViews({
  panes,
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
      if (!body) continue; // its cell has not mounted yet; a later pass catches it

      const term = new Terminal({
        fontFamily: getComputedStyle(document.body).fontFamily,
        fontSize: 12,
        theme: { background: "#0b0b0d", foreground: "#e6e6ec" },
        cursorBlink: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      term.onData((data) =>
        socketRef.current?.send(JSON.stringify({ type: "input", pane, data })),
      );
      // xterm parses OSC 0/2 window-title sequences; mirror the latest non-empty
      // one into the cell title. An empty title is ignored so the previous label
      // (or the "term N" fallback) stands, matching the TUI.
      term.onTitleChange((title) => {
        const cleaned = title.replace(/\s+/g, " ").trim();
        if (!cleaned) return;
        setTitles((current) => ({ ...current, [pane]: cleaned }));
      });
      term.open(body);
      viewsRef.current.set(pane, { term, fit });

      // Flush any output (typically replayed scrollback) that arrived before
      // this view existed, in order, so the restored screen is complete.
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
  }, [panes]);
}