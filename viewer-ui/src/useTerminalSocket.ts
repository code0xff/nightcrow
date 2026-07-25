import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { reconcileOrder } from "./paneOrder";
import { toast } from "./toast";
import type { PaneView } from "./terminalLayout";

interface UseTerminalSocketArgs {
  repo: string;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  pendingRef: MutableRefObject<Map<number, Uint8Array[]>>;
  sentSizesRef: MutableRefObject<Map<number, { rows: number; cols: number }>>;
  lastActiveByRepoRef: MutableRefObject<Map<string, number>>;
  expectCreateRef: MutableRefObject<number>;
  setPanes: React.Dispatch<React.SetStateAction<number[]>>;
  setActive: React.Dispatch<React.SetStateAction<number | null>>;
  setZoomed: React.Dispatch<React.SetStateAction<number | null>>;
  setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>>;
}

/// One WebSocket multiplexes every terminal for a repository. Pane ids belong
/// to a repository's own terminal hub, so switching repos must reset the pane
/// list and dispose the old terminals — otherwise stale ids point at panes the
/// new repo never created.
export function useTerminalSocket({
  repo,
  socketRef,
  viewsRef,
  pendingRef,
  sentSizesRef,
  lastActiveByRepoRef,
  expectCreateRef,
  setPanes,
  setActive,
  setZoomed,
  setTitles,
}: UseTerminalSocketArgs) {
  useEffect(() => {
    let closedByUs = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;

    const disposeAll = () => {
      viewsRef.current.forEach((view) => view.term.dispose());
      viewsRef.current.clear();
      pendingRef.current.clear();
      sentSizesRef.current.clear();
    };

    const connect = () => {
      // Each (re)connection starts from a clean slate and lets the server
      // repopulate it: on connect the hub replays every live pane and its
      // scrollback, so a browser refresh restores the terminals while a server
      // restart (no panes to replay) correctly comes back empty. Keeping stale
      // local panes would instead point at terminals the new socket never
      // announced.
      setPanes([]);
      setActive(null);
      setZoomed(null);
      setTitles({});
      disposeAll();

      const scheme = location.protocol === "https:" ? "wss:" : "ws:";
      const socket = new WebSocket(
        `${scheme}//${location.host}/ws/term?repo=${encodeURIComponent(repo)}`,
      );
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;

      socket.onmessage = (event) => {
        if (typeof event.data === "string") {
          const message = JSON.parse(event.data);
          if (message.type === "created") {
            const pane = message.pane;
            setPanes((current) => [...current, pane]);
            if (expectCreateRef.current > 0) {
              // A terminal this client just asked for: focus follows creation.
              expectCreateRef.current -= 1;
              setActive(pane);
              lastActiveByRepoRef.current.set(repo, pane);
            } else if (lastActiveByRepoRef.current.get(repo) === pane) {
              // A replayed pane that was focused before switching away — restore
              // it rather than letting focus land on the last replayed pane.
              setActive(pane);
            }
          } else if (message.type === "exited") {
            setPanes((current) => current.filter((p) => p !== message.pane));
            setActive((current) => (current === message.pane ? null : current));
            setZoomed((current) => (current === message.pane ? null : current));
            pendingRef.current.delete(message.pane);
            sentSizesRef.current.delete(message.pane);
            setTitles((current) => {
              if (!(message.pane in current)) return current;
              const next = { ...current };
              delete next[message.pane];
              return next;
            });
          } else if (message.type === "reordered") {
            // The hub's canonical order after a drag — this client's or another
            // device's. Adopt it, reconciled against the panes we actually hold
            // so a "created"/"exited" that raced it cannot desync the grid.
            // active/zoomed are pane ids, so they survive the reorder untouched.
            setPanes((current) => reconcileOrder(current, message.order));
          } else if (message.type === "error") {
            // A create was refused (e.g. the per-repo cap); do not let the
            // pending focus-follow attach to an unrelated later "created".
            expectCreateRef.current = 0;
            toast.error(message.message);
          }
          return;
        }
        const frame = new Uint8Array(event.data as ArrayBuffer);
        if (frame.length < 4) return;
        const pane = new DataView(frame.buffer).getUint32(0, true);
        const bytes = frame.subarray(4);
        const view = viewsRef.current.get(pane);
        if (view) {
          view.term.write(bytes);
        } else {
          // The view is created by a later effect; hold this until then.
          const queue = pendingRef.current.get(pane) ?? [];
          queue.push(bytes);
          pendingRef.current.set(pane, queue);
        }
      };
      // Reconnect quietly. The control socket is always open — it is how a
      // terminal gets created — so a drop with nothing running is not worth
      // alarming the user about; just wait and retry. A restart thus heals
      // into a clean, empty panel rather than a stuck error.
      socket.onclose = () => {
        if (closedByUs) return;
        reconnectTimer = setTimeout(connect, 1000);
      };
    };

    connect();

    return () => {
      closedByUs = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      socketRef.current?.close();
      disposeAll();
    };
  }, [repo]);
}