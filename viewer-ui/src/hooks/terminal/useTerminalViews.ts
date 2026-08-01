import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { PaneView } from "../../lib/terminalLayout";
import { terminalFontOptions } from "../../lib/termFont";
import { ClearKeyProbe } from "../../lib/clearKeyProbe";
import { sendTerminalMessage } from "../../api/terminal";

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
        ...terminalFontOptions(),
        theme: { background: "#0b0b0d", foreground: "#e6e6ec" },
        cursorBlink: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      // Why the probe is here: `lib/clearKeyProbe.ts`. It observes and reports;
      // the handler always returns true, so xterm keeps handling every key
      // exactly as it did.
      const probe = new ClearKeyProbe();
      term.attachCustomKeyEventHandler((event) => {
        probe.noteKey(event, performance.now());
        return true;
      });
      term.onData((data) => {
        const report = probe.report(data, performance.now());
        if (
          sendTerminalMessage(socketRef.current, { type: "input", pane, data }) &&
          report
        ) {
          sendTerminalMessage(socketRef.current, {
            type: "clear_key_report",
            pane,
            ...report,
          });
        }
      });
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
        // A replayed pane is history, and what a person wants from history is
        // its end — the last thing the program said, not wherever the write
        // happened to leave the viewport.
        //
        // Queued behind the replay rather than run after the loop: `write`
        // parses on a later task, so scrolling here directly would run while
        // the parser was still catching up and land on a buffer that had not
        // finished growing.
        term.write("", () => term.scrollToBottom());
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
