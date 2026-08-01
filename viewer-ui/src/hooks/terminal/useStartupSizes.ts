import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { terminalFontOptions } from "../../lib/termFont";
import {
  sendTerminalMessage,
  type PaneSize,
} from "../../api/terminal";

interface UseStartupSizesArgs {
  /** How many startup terminals the server is holding, or null when there is
   *  nothing to answer. */
  pending: number | null;
  /** Re-run when the layout moves, since a cell with no size cannot be
   *  measured yet. */
  size: { w: number; h: number };
  socketRef: MutableRefObject<WebSocket | null>;
  /** The placeholder cells rendered for the pending panes, by slot. */
  slotRefs: MutableRefObject<Map<number, HTMLDivElement>>;
  /** Whether a real pane already exists. If one does, the placeholders are
   *  gone and there is nothing left to measure — see the answer below. */
  panesExist: boolean;
  onAnswered: () => void;
}

/**
 * Answer the server's `pending` offer with the size of each startup terminal.
 *
 * The measurement is taken from the real cell rather than computed from the
 * grid: a placeholder cell is rendered in the slot the pane will occupy, and a
 * throwaway terminal opened into it reports the rows and cols that element
 * holds. Arithmetic over the grid template would have to re-derive the gaps and
 * the cell header, and any drift there produces exactly the wrong-size birth
 * this handshake exists to prevent.
 */
export function useStartupSizes({
  pending,
  size,
  socketRef,
  slotRefs,
  panesExist,
  onAnswered,
}: UseStartupSizesArgs) {
  useEffect(() => {
    if (pending === null) return;
    const socket = socketRef.current;
    if (!socket || socket.readyState !== WebSocket.OPEN) return;

    const slots: HTMLDivElement[] = [];
    for (let slot = 0; slot < pending; slot++) {
      const node = slotRefs.current.get(slot);
      if (!node || node.clientHeight === 0 || node.clientWidth === 0) {
        // Creating a terminal by hand before the handshake finished replaces
        // the placeholders with real cells, so there is nothing left to
        // measure and nothing that will re-render them. Answer anyway — the
        // server opens the startup terminals at its default — rather than
        // leave them unclaimed for the life of the hub.
        if (!panesExist) return; // Just not laid out yet; a later pass retries.
        if (sendTerminalMessage(socket, { type: "start", sizes: [] })) {
          onAnswered();
        }
        return;
      }
      slots.push(node);
    }

    let sizes: PaneSize[] = [];
    try {
      sizes = slots.map((node) => {
        const term = new Terminal(terminalFontOptions());
        const fit = new FitAddon();
        term.loadAddon(fit);
        term.open(node);
        const proposed = fit.proposeDimensions();
        term.dispose();
        if (!proposed) throw new Error("could not measure the cell");
        return { rows: proposed.rows, cols: proposed.cols };
      });
    } catch {
      // The fallback belongs here, not in a server-side timeout: this side is
      // the one that knows the measurement failed, so it answers with what it
      // has. The server then opens the rest at its default and the first fit
      // corrects them — the behaviour every pane had before.
      sizes = [];
    }

    if (sendTerminalMessage(socket, { type: "start", sizes })) onAnswered();
  }, [pending, size, socketRef, slotRefs, panesExist, onAnswered]);
}
