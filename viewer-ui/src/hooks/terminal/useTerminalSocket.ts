import { useLayoutEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import type { RecoveryByPane } from "../../lib/recovery";
import { takeClaim, viewerId } from "../../lib/viewerId";
import type { PaneView } from "../../lib/terminalLayout";
import type { PaneSize } from "../../api/terminal";
import { handleTerminalSocketMessage } from "./terminalSocketMessages";

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
    const messageContext = {
      repo,
      clientIdRef,
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
    };

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
        handleTerminalSocketMessage(event.data, messageContext);
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
