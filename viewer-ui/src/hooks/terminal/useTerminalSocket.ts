import { useLayoutEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import { reconcileOrder } from "../../lib/paneOrder";
import { applyRecovery, type RecoveryByPane } from "../../lib/recovery";
import { toast } from "../../lib/toast";
import { takeClaim, viewerId } from "../../lib/viewerId";
import type { PaneView } from "../../lib/terminalLayout";
import {
  decodeTerminalControlFrame,
  decodeTerminalOutputFrame,
  type PaneSize,
} from "../../api/terminal";

interface UseTerminalSocketArgs {
  repo: string;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  pendingRef: MutableRefObject<Map<number, Uint8Array[]>>;
  sentSizesRef: MutableRefObject<Map<number, PaneSize>>;
  lastActiveByRepoRef: MutableRefObject<Map<string, number>>;
  /** What the page last asked the zoom to be (see `usePaneCommands`). Cleared
   *  here because this is what knows when a request has been answered and when
   *  the connection carrying it is gone — including a repository switch, whose
   *  pane ids belong to a different project entirely. */
  zoomAskedRef: MutableRefObject<number | null | undefined>;
  setPending: React.Dispatch<React.SetStateAction<number | null>>;
  /** Panes the replay has promised and not yet delivered. */
  setReplayLeft: React.Dispatch<React.SetStateAction<number>>;
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
  zoomAskedRef,
  setPending,
  setReplayLeft,
  setPanes,
  setActive,
  setZoomed,
  setTitles,
  setOwnsSize,
  setRecovery,
}: UseTerminalSocketArgs) {
  // Who the hub calls this connection, so a `created` naming a requester can be
  // read as this page's or somebody else's. Minted per connection, so it is
  // cleared with the socket rather than with the project.
  const clientIdRef = useRef<number | null>(null);

  useLayoutEffect(() => {
    let closedByUs = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;

    const disposeAll = () => {
      viewsRef.current.forEach((view) => view.term.dispose());
      viewsRef.current.clear();
      pendingRef.current.clear();
      sentSizesRef.current.clear();
    };

    const connect = () => {
      clientIdRef.current = null;
      setReplayLeft(0);
      // Anything asked for on the socket that just went is unanswerable, and a
      // switch has moved to pane ids that mean something else.
      zoomAskedRef.current = undefined;
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
          const message = decodeTerminalControlFrame(event.data);
          if (!message) return;
          if (message.type === "hello") {
            // The first frame on a connection, ahead of anything that names a
            // requester and ahead of the panes it counts.
            clientIdRef.current = message.client;
            setReplayLeft(message.panes);
          } else if (message.type === "pending") {
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
            const title = message.title;
            if (title) {
              setTitles((current) => ({ ...current, [pane]: title }));
            }
            setPanes((current) => [...current, pane]);
            // One of the replayed panes has landed, so the grid holds one fewer
            // cell open. Counted rather than compared against the pane list: a
            // pane exiting mid-replay must not leave a cell open forever.
            setReplayLeft((left) => (left > 0 ? left - 1 : 0));
            // Focus it only if this page is the one that asked. The hub stamps
            // every pane with its requester and told us our own id on connect
            // (`hello`), so two pages creating at once each take their own —
            // where counting outstanding creates took whichever came back
            // first, which could be the other page's terminal.
            if (message.client != null && message.client === clientIdRef.current) {
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
            // The server has spoken, so what this page asked for is spent.
            zoomAskedRef.current = undefined;
            // Which pane fills the panel is the repository's answer, so this is
            // the only thing that sets it — including for the page that asked.
            // Sent on connect too, which is what brings a reloaded page back to
            // the zoom it left. `null` is a value here, not an absent field:
            // it is how the panel is told to go back to the grid.
            setZoomed(message.pane ?? null);
          } else if (message.type === "error") {
            toast.error(message.message);
          } else {
            const unreachable: never = message;
            return unreachable;
          }
          return;
        }
        if (!(event.data instanceof ArrayBuffer)) return;
        const frame = decodeTerminalOutputFrame(event.data);
        if (!frame) return;
        const { pane, data: bytes } = frame;
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
