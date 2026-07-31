import { useLayoutEffect } from "react";
import type { MutableRefObject } from "react";
import { reconcileOrder } from "../../lib/paneOrder";
import { applyRecovery, type RecoveryByPane } from "../../lib/recovery";
import { toast } from "../../lib/toast";
import { takeClaim, viewerId } from "../../lib/viewerId";
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
  /** Whether this page's layout is what sets the pane sizes. */
  setOwnsSize: React.Dispatch<React.SetStateAction<boolean>>;
  setRecovery: React.Dispatch<React.SetStateAction<RecoveryByPane>>;
}

/// Reset state on repository changes because pane ids are repository-local.
///
/// A layout effect, not a passive one: the panel is not remounted per
/// repository (it keeps the per-repo focus memory across switches), so the
/// render that switches project still commits the previous project's panes and
/// their xterm DOM. A passive effect may run after that has been painted, which
/// puts one frame of the old project's terminals on screen; a layout effect
/// clears them before the browser paints.
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
  setOwnsSize,
  setRecovery,
}: UseTerminalSocketArgs) {
  useLayoutEffect(() => {
    // A terminal this page asked for belongs to the project it was asked in.
    // The next project replays the panes it already has, and an expectation
    // left over from the previous one would take the first of them for the
    // terminal that never arrived — focusing it and remembering it as this
    // project's last active pane. Cleared here rather than in `connect` so a
    // reconnect, which is the same project, still adopts the pane it asked for.
    expectCreateRef.current = 0;
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
      // Only a page someone just opened takes the sizing, and only then is it
      // worth assuming rather than awaiting — starting as a spectator would
      // leave that page's panes unfitted for a round trip. A switch or a
      // reconnect keeps whatever this page already had; the server confirms it
      // either way.
      const arriving = takeClaim();
      if (arriving) setOwnsSize(true);
      // Reports are keyed by pane id, which is repository-local.
      setRecovery({});
      disposeAll();

      const scheme = location.protocol === "https:" ? "wss:" : "ws:";
      // The page names itself, so the session can tell one screen's sockets
      // coming and going from a new screen arriving.
      const query = new URLSearchParams({ repo, viewer: viewerId() });
      if (arriving) query.set("claim", "1");
      const socket = new WebSocket(
        `${scheme}//${location.host}/ws/term?${query}`,
      );
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;

      socket.onmessage = (event) => {
        // Pane ids are per repository, so a frame that was already on its way
        // when the project changed would land on whatever pane holds that id
        // here. Only the live socket may touch this state.
        if (socketRef.current !== socket) return;
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
            // The session names a configured startup terminal, and calls it the
            // same thing in every client. A pane this page asked for has no
            // name until the program in it sets one.
            if (typeof message.title === "string" && message.title) {
              setTitles((current) => ({ ...current, [pane]: message.title }));
            }
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
            // The zoom is not cleared here: a pane leaving ends it on the
            // server, which says so in a `zoomed` of its own. Doing it locally
            // as well would be this page answering a question it does not own.
            pendingRef.current.delete(message.pane);
            sentSizesRef.current.delete(message.pane);
            setTitles((current) => {
              if (!(message.pane in current)) return current;
              const next = { ...current };
              delete next[message.pane];
              return next;
            });
          } else if (message.type === "resized") {
            // The size the PTY is now set to, which this page may not have
            // asked for: one client at a time decides it. Adopted either way —
            // the emulator has to wrap where the child does.
            sentSizesRef.current.set(message.pane, {
              rows: message.rows,
              cols: message.cols,
            });
            viewsRef.current
              .get(message.pane)
              ?.term.resize(message.cols, message.rows);
          } else if (message.type === "recovery") {
            // Deliberately survives the pane's `exited`: the report that matters
            // most arrives while the process is gone and its slot is held for a
            // relaunch. The server's own `cancelled` report is what clears it.
            setRecovery((current) => applyRecovery(current, message));
          } else if (message.type === "size_owner") {
            setOwnsSize(message.owned);
          } else if (message.type === "reordered") {
            setPanes((current) => reconcileOrder(current, message.order));
          } else if (message.type === "zoomed") {
            // Which pane fills the panel is the repository's answer, so this is
            // the only thing that sets it — including for the page that asked.
            // Sent on connect too, which is what brings a reloaded page back to
            // the zoom it left. `null` is a value here, not an absent field:
            // it is how the panel is told to go back to the grid.
            setZoomed(message.pane ?? null);
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
