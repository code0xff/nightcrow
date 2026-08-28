import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { PaneView } from "../../lib/terminalLayout";
import type { PaneViewMode } from "../../lib/paneViewMode";
import { terminalFontOptions } from "../../lib/termFont";
import { ClearKeyProbe } from "../../lib/clearKeyProbe";
import { overriddenKeySequence } from "../../lib/hardwareKeys";
import { OSC_CLIPBOARD } from "../../lib/osc52";
import { receivePaneClipboard } from "../../lib/paneClipboard";
import { sendTerminalMessage, type PaneSize } from "../../api/terminal";

interface UseTerminalViewsArgs {
  panes: number[];
  // Opening xterm inside a hidden (display:none) cell caches a 0x0
  // character-cell measurement that fit() can never recover, so creation waits
  // until the cell has size. Re-run on the layout changes that can reveal a
  // cell (`size`, `zoomed`, `mode`), mirroring the fit effect's dependency set.
  size: { w: number; h: number };
  zoomed: number | null;
  mode: PaneViewMode;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  bodyRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  pendingRef: MutableRefObject<Map<number, Uint8Array[]>>;
  /** What each PTY's grid is, from `created` and `resized`. Read here as the
   *  size a pane's replay has to be parsed at. */
  ptySizesRef: MutableRefObject<Map<number, PaneSize>>;
  /** What the key bar's Ctrl latch makes of a typed character (`useCtrlLatch`).
   *  Every pane's input passes through it, because the latch belongs to the bar
   *  rather than to a pane and the next character may be typed into any of
   *  them. */
  consumeCtrl: (typed: string) => string;
  setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>>;
}

export function useTerminalViews({
  panes,
  size,
  zoomed,
  mode,
  socketRef,
  viewsRef,
  bodyRefs,
  pendingRef,
  ptySizesRef,
  consumeCtrl,
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
        // Without this there is no way to select text on a Mac in a pane whose
        // program reads the mouse — which is most of what this panel is opened
        // for. xterm offers one way back: a modifier that forces the drag to be
        // a selection (Shift, or Option on a Mac only if this is set). Left
        // unset, Cmd+C copies nothing and nothing explains why.
        macOptionClickForcesSelection: true,
      });
      const fit = new FitAddon();
      term.loadAddon(fit);
      // Why the probe is here: `lib/clearKeyProbe.ts`. It observes and reports;
      // the handler always returns true, so xterm keeps handling every key
      // exactly as it did.
      const probe = new ClearKeyProbe();
      term.attachCustomKeyEventHandler((event) => {
        probe.noteKey(event, performance.now());
        const overridden = overriddenKeySequence(event);
        if (overridden === null) return true;
        // Back through xterm's own input path rather than straight to the
        // socket, so the bytes meet everything a typed key does — the probe's
        // report, the Ctrl latch — and the pane scrolls to the bottom the way
        // typing into it does.
        term.input(overridden);
        return false;
      });
      term.onData((data) => {
        // The probe is asked about what was typed, not about what the latch
        // turns it into. It pairs a clear byte with the keydown behind it, and a
        // byte the bar made has no such keydown — it would be filed as "arrived
        // with no key event", which is the shape of the thing being hunted. The
        // bar's own `^L` is already outside the probe for the same reason: it
        // never goes through a terminal at all.
        const report = probe.report(data, performance.now());
        const input = consumeCtrl(data);
        if (
          sendTerminalMessage(socketRef.current, {
            type: "input",
            pane,
            data: input,
          }) &&
          report
        ) {
          sendTerminalMessage(socketRef.current, {
            type: "clear_key_report",
            pane,
            ...report,
          });
        }
      });
      // A program in the pane asks for the clipboard this way, and it is the
      // only path that reaches whoever is reading — the host's own clipboard
      // is a different machine's whenever this panel is open from somewhere
      // else. See `lib/osc52.ts`. Returning true claims the sequence so it is
      // not also treated as unrecognised output; the handler runs async work
      // beside the parse rather than inside it, which would hold the stream
      // — and the pane's painting — behind a clipboard permission prompt.
      term.parser.registerOscHandler(OSC_CLIPBOARD, (payload) => {
        void receivePaneClipboard(payload).catch((error: unknown) => {
          // Dropping the promise is the point; dropping a rejection with it
          // would make a DOM failure here an unhandled rejection with no name
          // on it.
          console.error("nightcrow: a pane's clipboard request failed", error);
        });
        return true;
      });
      // Preserve the previous label when OSC provides an empty title.
      term.onTitleChange((title) => {
        const cleaned = title.replace(/\s+/g, " ").trim();
        if (!cleaned) return;
        setTitles((current) => ({ ...current, [pane]: cleaned }));
      });
      term.open(body);
      // Before a byte is written: what follows was drawn against the PTY's
      // grid, and xterm opens at its own 80×24 default. Parsed at the default,
      // everything the program put outside that box is gone — and the fit that
      // runs after this sends nothing when it lands on the size the PTY already
      // has (the usual outcome of returning to the same screen), so what was
      // lost stays lost until the program next draws by itself.
      //
      // The pane's grid *now*, even for output that has been queueing since an
      // older one — a cell can stay sizeless long enough to be resized under
      // it. Splitting the queue at each resize is not the improvement it
      // sounds like: `write` parses on a later task while `resize` applies at
      // once, so honouring a boundary would need an emulator exclusive to one
      // write loop, which it cannot be.
      //
      // That is also the limit of what this line promises — the grid the
      // replay is *handed to*, not one it is guaranteed to be read at. In the
      // case this is here for (a screen returning to panes it already sized)
      // the fit lands on the size the PTY already has and changes nothing,
      // which is exactly why nothing else was going to correct the default.
      const pty = ptySizesRef.current.get(pane);
      if (pty) term.resize(pty.cols, pty.rows);
      viewsRef.current.set(pane, { term, fit });

      const queued = pendingRef.current.get(pane);
      if (queued) {
        for (const chunk of queued) term.write(chunk);
        pendingRef.current.delete(pane);
        // A replayed pane is history, and what a person wants from history is
        // its end. Queued behind the replay rather than run after the loop:
        // `write` parses on a later task, so scrolling here directly would
        // land on a buffer that had not finished growing.
        term.write("", () => term.scrollToBottom());
      }
    }

    for (const [pane, view] of viewsRef.current) {
      if (!panes.includes(pane)) {
        view.term.dispose();
        viewsRef.current.delete(pane);
      }
    }
  }, [panes, size, zoomed, mode]);
}
