import { useCallback } from "react";
import type { LinkState } from "../../lib/attachStatus";
import type { PaneViewMode } from "../../lib/paneViewMode";
import type { RecoveryByPane } from "../../lib/recovery";
import { zoomPending } from "../../lib/zoom";
import type { TerminalRefs } from "./useTerminalRefs";
import { usePaneFocus } from "./usePaneFocus";
import { usePaneSizes } from "./usePaneSizes";
import { useStartupSizes } from "./useStartupSizes";
import { useTerminalSocket } from "./useTerminalSocket";
import { useTerminalViews } from "./useTerminalViews";

// How the panel is wired to the session: the socket, the xterms it fills, and
// the two things that follow from a layout — what size each PTY is asked for and
// which pane holds the keyboard.
//
// Lifted out of `TerminalPanel` unchanged, in the order it was written, because
// effects run in the order their hooks were called and this one is load-bearing:
// the socket subscribes before the views it feeds exist, and both come before
// anything that measures a cell. Nothing here decides anything — the component
// still owns every piece of state below and every callback the render uses.

export interface TerminalWiringArgs {
  repo: string;
  refs: TerminalRefs;
  /** The panel's measured box, re-read on every layout move. */
  size: { w: number; h: number };
  mode: PaneViewMode;
  panes: number[];
  active: number | null;
  /** Panes the replay has promised and not yet delivered. */
  replayLeft: number;
  /** Startup terminals the server is holding, or null with nothing to answer. */
  pending: number | null;
  ownsSize: boolean;
  /** The zoom actually rendered, and the raw one the server sent. A tabbed panel
   *  renders neither — see the comment on `zoomShown` in `Terminal.tsx`. */
  zoomShown: number | null;
  zoomServer: number | null;
  consumeCtrl: (typed: string) => string;
  setLink: React.Dispatch<React.SetStateAction<LinkState>>;
  setPending: React.Dispatch<React.SetStateAction<number | null>>;
  setReplayLeft: React.Dispatch<React.SetStateAction<number>>;
  setPanes: React.Dispatch<React.SetStateAction<number[]>>;
  setActive: React.Dispatch<React.SetStateAction<number | null>>;
  setZoomed: React.Dispatch<React.SetStateAction<number | null>>;
  setTitles: React.Dispatch<React.SetStateAction<Record<number, string>>>;
  setOwnsSize: React.Dispatch<React.SetStateAction<boolean>>;
  setRecovery: React.Dispatch<React.SetStateAction<RecoveryByPane>>;
}

export function useTerminalWiring({
  repo,
  refs,
  size,
  mode,
  panes,
  active,
  replayLeft,
  pending,
  ownsSize,
  zoomShown,
  zoomServer,
  consumeCtrl,
  setLink,
  setPending,
  setReplayLeft,
  setPanes,
  setActive,
  setZoomed,
  setTitles,
  setOwnsSize,
  setRecovery,
}: TerminalWiringArgs): void {
  const {
    containerRef,
    socketRef,
    viewsRef,
    bodyRefs,
    ptySizesRef,
    askedSizesRef,
    pendingRef,
    zoomAskedRef,
    slotRefs,
  } = refs;

  useTerminalSocket({
    repo,
    socketRef,
    viewsRef,
    pendingRef,
    ptySizesRef,
    askedSizesRef,
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
  });

  useTerminalViews({
    panes,
    size,
    zoomed: zoomShown,
    mode,
    socketRef,
    viewsRef,
    bodyRefs,
    pendingRef,
    ptySizesRef,
    consumeCtrl,
    setTitles,
  });

  const onAnswered = useCallback(() => setPending(null), [setPending]);
  useStartupSizes({
    pending,
    size,
    socketRef,
    slotRefs,
    panesExist: panes.length > 0,
    onAnswered,
  });

  usePaneSizes({
    panes,
    size,
    zoomed: zoomShown,
    mode,
    socketRef,
    viewsRef,
    bodyRefs,
    ptySizesRef,
    askedSizesRef,
    ownsSize,
    layoutPending: zoomPending(zoomServer, panes),
  });

  usePaneFocus({
    repo,
    panes,
    replayLeft,
    active,
    setActive,
    zoomed: zoomServer,
    zoom: zoomShown,
    viewsRef,
    panelRef: containerRef,
    size,
    mode,
  });
}
