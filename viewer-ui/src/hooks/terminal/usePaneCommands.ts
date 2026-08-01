import type { MutableRefObject } from "react";
import type { PaneView } from "../../lib/terminalLayout";
import { TERM_KEY_BAR, termKeySequence } from "../../lib/termKeys";

interface UsePaneCommandsArgs {
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  /** The pane filling the panel, needed to know what a toggle should ask for. */
  zoomed: number | null;
  /** The pane a key from the on-screen bar is typed into. */
  active: number | null;
}

/**
 * What this page asks the session to do: every message it sends that is not a
 * resize (which follows the layout, in `usePaneSizes`) or a drag (which has its
 * own hook).
 *
 * None of these change what is on screen. The hub answers each with a broadcast
 * and the client renders that — so two pages, and an attached TUI, converge on
 * one arrangement instead of each applying its own and drifting. A command sent
 * while the socket is down is dropped rather than queued: it would arrive after
 * a reconnect that has already replayed the session's real state, and undo it.
 */
export function usePaneCommands({
  socketRef,
  viewsRef,
  zoomed,
  active,
}: UsePaneCommandsArgs) {
  const send = (message: unknown) =>
    socketRef.current?.send(JSON.stringify(message));

  // The pane that comes back names this connection as its requester, which is
  // how the socket hook knows to focus it (`hello`).
  const create = () => send({ type: "create", rows: 24, cols: 80 });

  // Asked for, not applied. The zoom belongs to the repository rather than to
  // this page — the server keeps it, so a reload comes back to it — so this
  // waits for the echo like a reorder does, and every other client turns with
  // it. Ending the zoom when a pane opens is the server's job too: it knows
  // about panes this page did not ask for.
  const toggleZoom = (pane: number) =>
    send({ type: "zoom", pane: zoomed === pane ? null : pane });

  // Take the sizing back. Deliberate rather than automatic: the panes belong to
  // a session someone else may be working in, and merely opening this page must
  // not repaint their screen.
  const claimSize = () => send({ type: "claim_size" });

  const closePane = (pane: number) => send({ type: "close", pane });

  const reorder = (order: number[]) => send({ type: "reorder", order });

  const sendKey = (key: (typeof TERM_KEY_BAR)[number]["key"]) => {
    if (active === null) return;
    // The same key is different bytes depending on what the program has put the
    // terminal in, so it is read off the emulator rather than assumed.
    const appCursor =
      viewsRef.current.get(active)?.term.modes.applicationCursorKeysMode ??
      false;
    send({ type: "input", pane: active, data: termKeySequence(key, appCursor) });
  };

  return { create, toggleZoom, claimSize, closePane, reorder, sendKey };
}
