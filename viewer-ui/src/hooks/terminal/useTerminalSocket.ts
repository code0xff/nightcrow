import { useLayoutEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import type { LinkState } from "../../lib/terminal/attachStatus";
import type { RecoveryByPane } from "../../lib/terminal/recovery";
import { RECONNECT_STABLE_MS, reconnectDelay } from "../../lib/terminal/reconnect";
import { viewerId } from "../../lib/runtime/viewerId";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import type { PaneSize } from "../../api/terminal";
import {
  resetTerminalWrites,
  terminalWrites,
} from "../../lib/terminal/terminalWrites";
import type {
  PendingTerminalWrite,
  TerminalReplayState,
} from "../../lib/terminal/terminalReplay";
import { handleTerminalSocketMessage } from "./terminalSocketMessages";

interface UseTerminalSocketArgs {
  repo: string;
  socketRef: MutableRefObject<WebSocket | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  pendingRef: MutableRefObject<Map<number, PendingTerminalWrite[]>>;
  replayRef: MutableRefObject<TerminalReplayState>;
  ptySizesRef: MutableRefObject<Map<number, PaneSize>>;
  askedSizesRef: MutableRefObject<Map<number, PaneSize>>;
  desiredSizesRef: MutableRefObject<Map<number, PaneSize>>;
  ownsSizeRef: MutableRefObject<boolean>;
  sizeOwnerGenerationRef: MutableRefObject<string | null>;
  onSizeAcquired: () => void;
  /** What the page last asked the zoom to be (see `usePaneCommands`). Cleared
   *  here because this is what knows when a request has been answered and when
   *  the connection carrying it is gone — including a repository switch, whose
   *  pane ids belong to a different project entirely. */
  zoomAskedRef: MutableRefObject<number | null | undefined>;
  /** Where the socket is, so the panel can say it is attaching rather than
   *  leave a blank panel reading as "no terminal open". */
  setLink: React.Dispatch<React.SetStateAction<LinkState>>;
  setPending: React.Dispatch<React.SetStateAction<number | null>>;
  /** Panes the replay has promised and not yet delivered. */
  setReplayLeft: React.Dispatch<React.SetStateAction<number>>;
  setPanes: React.Dispatch<React.SetStateAction<number[]>>;
  setActive: React.Dispatch<React.SetStateAction<number | null>>;
  setZoomed: React.Dispatch<React.SetStateAction<number | null>>;
  setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>>;
  /** Whether this page's layout is what sets the pane sizes. */
  setOwnsSize: (owned: boolean) => void;
  setRecovery: React.Dispatch<React.SetStateAction<RecoveryByPane>>;
}

/// Reset state on repository changes because pane ids are repository-local.
///
/// A layout effect, not a passive one: the panel is not remounted per
/// repository (it keeps the per-repo focus memory across switches), so the
/// render that switches project still commits the previous project's panes.
/// A passive effect may run after that has been painted, putting one frame of
/// the old project's terminals on screen; a layout effect clears them before
/// the browser paints.
export function useTerminalSocket({
  repo,
  socketRef,
  viewsRef,
  pendingRef,
  replayRef,
  ptySizesRef,
  askedSizesRef,
  desiredSizesRef,
  ownsSizeRef,
  sizeOwnerGenerationRef,
  onSizeAcquired,
  zoomAskedRef,
  setLink,
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
    // How long the current connection has to stay up before the next loss
    // starts over from the base wait.
    let stableTimer: ReturnType<typeof setTimeout> | undefined;
    // Retry number for the connection that is not up yet (`reconnect.ts`).
    let attempt = 0;
    // Whether this page has ever been attached to *this* repository's session.
    // A link lost after that is a reconnect, which is a different thing to be
    // told than a first attach; the effect restarts per repository, so
    // switching projects says "connecting" again rather than inheriting the
    // previous project's link.
    let everLive = false;
    // Whether a connection has been opened before in this effect run. A later
    // one is a reconnect to the same session, not a project switch: its panes
    // are the same panes and their emulators are reused.
    let attachedOnce = false;
    const linkTo = (state: LinkState) => {
      if (state === "live") everLive = true;
      setLink(state);
    };
    const waiting = (): LinkState => (everLive ? "reconnecting" : "connecting");
    const stopStableTimer = () => {
      if (stableTimer) clearTimeout(stableTimer);
      stableTimer = undefined;
    };
    const messageContext = {
      repo,
      clientIdRef,
      viewsRef,
      pendingRef,
      replayRef,
      ptySizesRef,
      askedSizesRef,
      desiredSizesRef,
      ownsSizeRef,
      sizeOwnerGenerationRef,
      onSizeAcquired,
      zoomAskedRef,
      setLink: linkTo,
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
      viewsRef.current.forEach((view) => {
        resetTerminalWrites(view.term);
        view.term.dispose();
      });
      viewsRef.current.clear();
      pendingRef.current.clear();
      replayRef.current.active = false;
      replayRef.current.panes.clear();
      replayRef.current.order.length = 0;
      ptySizesRef.current.clear();
      askedSizesRef.current.clear();
      desiredSizesRef.current.clear();
    };

    const connect = () => {
      stopStableTimer();
      const reuse = attachedOnce;
      attachedOnce = true;
      clientIdRef.current = null;
      ownsSizeRef.current = false;
      sizeOwnerGenerationRef.current = null;
      linkTo(waiting());
      setReplayLeft(0);
      // Anything asked for on the socket that just went is unanswerable, and a
      // switch has moved to pane ids that mean something else.
      zoomAskedRef.current = undefined;
      setPending(null);
      // Reports are keyed by pane id, which is repository-local.
      setRecovery({});
      if (reuse) {
        // Keep the emulators. A replay is an absolute repaint of each pane's
        // screen — the hub composes its full history and screen record — so a
        // reset gives it the empty buffer a fresh attach would have, without
        // rebuilding every pane's DOM, parser, and fit for bytes that are
        // already on their way. The reset is queued behind bytes the dead
        // connection left parsing, and the replay queues behind it, so nothing
        // from the old screen can land on the new one.
        for (const view of viewsRef.current.values()) {
          const writes = terminalWrites(view.term);
          writes.clear();
          writes.barrier(() => view.term.reset());
        }
        // Bytes held for a pane that had no cell yet belong to the dead
        // connection; the replay carries that pane's screen again.
        pendingRef.current.clear();
        replayRef.current.active = false;
        replayRef.current.panes.clear();
        replayRef.current.order.length = 0;
        ptySizesRef.current.clear();
        askedSizesRef.current.clear();
        desiredSizesRef.current.clear();
      } else {
        // A new repository (or the first attach): nothing on screen belongs to
        // it.
        setPanes([]);
        setActive(null);
        setZoomed(null);
        setTitles({});
        disposeAll();
      }

      const scheme = location.protocol === "https:" ? "wss:" : "ws:";
      // The page names itself, so the session can tell one screen's sockets
      // coming and going from a new screen arriving.
      const query = new URLSearchParams({ repo, viewer: viewerId() });
      const socket = new WebSocket(
        `${scheme}//${location.host}/ws/term?${query}`,
      );
      socket.binaryType = "arraybuffer";
      socketRef.current = socket;

      socket.onopen = () => {
        if (socketRef.current !== socket) return;
        stopStableTimer();
        stableTimer = setTimeout(() => {
          stableTimer = undefined;
          attempt = 0;
        }, RECONNECT_STABLE_MS);
      };
      socket.onmessage = (event) => {
        // Pane ids are per repository, so a frame that was already on its way
        // when the project changed would land on whatever pane holds that id
        // here. Only the live socket may touch this state.
        if (socketRef.current !== socket) return;
        handleTerminalSocketMessage(event.data, messageContext);
      };
      socket.onclose = () => {
        if (closedByUs) return;
        stopStableTimer();
        // Said here rather than left to `connect`: the panes of the socket that
        // just went stay on screen in the meantime, and typing into them
        // reaches nothing.
        linkTo(waiting());
        const delay = reconnectDelay(attempt);
        attempt += 1;
        reconnectTimer = setTimeout(connect, delay);
      };
    };

    connect();

    return () => {
      closedByUs = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      stopStableTimer();
      socketRef.current?.close();
      disposeAll();
    };
  }, [
    repo,
    socketRef,
    viewsRef,
    pendingRef,
    replayRef,
    ptySizesRef,
    askedSizesRef,
    desiredSizesRef,
    ownsSizeRef,
    sizeOwnerGenerationRef,
    onSizeAcquired,
    zoomAskedRef,
    setLink,
    setPending,
    setReplayLeft,
    setPanes,
    setActive,
    setZoomed,
    setTitles,
    setOwnsSize,
    setRecovery,
  ]);
}
