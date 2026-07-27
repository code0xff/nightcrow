import { useEffect } from "react";
import type { MutableRefObject } from "react";
import { reconcileOrder } from "../../lib/paneOrder";
import { toast } from "../../lib/toast";
import type { PaneView } from "../../lib/terminalLayout";

interface UseTerminalSocketArgs {
  repo: string;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  pendingRef: MutableRefObject<Map<number, Uint8Array[]>>;
  sentSizesRef: MutableRefObject<Map<number, { rows: number; cols: number }>>;
  lastActiveByRepoRef: MutableRefObject<Map<string, number>>;
  expectCreateRef: MutableRefObject<number>;
  setPending: React.Dispatch<React.SetStateAction<number | null>>;
  setPanes: React.Dispatch<React.SetStateAction<number[]>>;
  setActive: React.Dispatch<React.SetStateAction<number | null>>;
  setZoomed: React.Dispatch<React.SetStateAction<number | null>>;
  setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>>;
}

/// Reset state on repository changes because pane ids are repository-local.
export function useTerminalSocket({
  repo,
  socketRef,
  viewsRef,
  pendingRef,
  sentSizesRef,
  lastActiveByRepoRef,
  expectCreateRef,
  setPending,
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
      setPending(null);
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
          if (message.type === "pending") {
            // Startup terminals the server is holding until this page says how
            // big to make them. Answered by `useStartupSizes`.
            setPending(message.count);
          } else if (message.type === "created") {
            const pane = message.pane;
            // Adopt the size the PTY already has. Without this the first fit
            // sends a resize even when it computes the very size the pane is
            // already set to — and every resize costs the child a full
            // repaint, which is the flicker a reload used to show.
            sentSizesRef.current.set(pane, {
              rows: message.rows,
              cols: message.cols,
            });
            setPanes((current) => [...current, pane]);
            if (expectCreateRef.current > 0) {
              expectCreateRef.current -= 1;
              setActive(pane);
              lastActiveByRepoRef.current.set(repo, pane);
            } else if (lastActiveByRepoRef.current.get(repo) === pane) {
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
            setPanes((current) => reconcileOrder(current, message.order));
          } else if (message.type === "error") {
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
          const queue = pendingRef.current.get(pane) ?? [];
          queue.push(bytes);
          pendingRef.current.set(pane, queue);
        }
      };
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
