import { useRef } from "react";
import { useRepoPoll, type UseRepoPollResult } from "./useRepoPoll";
import { useRepoOrder } from "./useRepoOrder";

export interface UseProjectTabsArgs {
  authed: boolean | null;
  setAuthed: React.Dispatch<React.SetStateAction<boolean | null>>;
  handle: (err: unknown) => void;
  resumeTick: number;
  adoptAccent: (accent: number) => void;
  adoptSidebarWidth: (px: number) => void;
  accentWrites: React.MutableRefObject<number>;
  sidebarWrites: React.MutableRefObject<number>;
  /** True while the sidebar divider is being dragged, so a poll does not adopt
   *  a width the user is still choosing. */
  draggingRef: React.MutableRefObject<boolean>;
}

/**
 * The project tab strip: which repositories are open, and their order.
 *
 * Polling and reordering are one unit because they contend for the same
 * state. Four refs pass between them — a reorder in flight, one queued behind
 * it, a drag in progress, and a count of local order writes — and each exists
 * so the poll can tell "the server's order is news" from "the server has not
 * caught up with what this page just did". Owning them here keeps that
 * handshake in one file instead of spread across the call site, where a
 * missing ref would surface as tabs snapping back mid-drag.
 */
export function useProjectTabs({
  authed,
  setAuthed,
  handle,
  resumeTick,
  adoptAccent,
  adoptSidebarWidth,
  accentWrites,
  sidebarWrites,
  draggingRef,
}: UseProjectTabsArgs): UseRepoPollResult & {
  orderWrites: React.MutableRefObject<number>;
  draggingRepo: string | null;
  dragOverRepo: string | null;
  onRepoDragStart: (event: React.PointerEvent, id: string) => void;
  onRepoDragMove: (event: React.PointerEvent) => void;
  onRepoDragEnd: () => void;
} {
  const orderWrites = useRef(0);
  const repoDraggingRef = useRef(false);
  const reorderInFlightRef = useRef(false);
  const pendingReorderRef = useRef<string[] | null>(null);

  const poll = useRepoPoll({
    authed,
    setAuthed,
    handle,
    adoptAccent,
    adoptSidebarWidth,
    draggingRef,
    accentWrites,
    sidebarWrites,
    resumeTick,
    orderWrites,
    repoDraggingRef,
    reorderInFlightRef,
    pendingReorderRef,
  });

  const {
    dragging: draggingRepo,
    target: dragOverRepo,
    onStart: onRepoDragStart,
    onMove: onRepoDragMove,
    onEnd: onRepoDragEnd,
  } = useRepoOrder({
    repos: poll.repos,
    setRepos: poll.setRepos,
    handle,
    writesRef: orderWrites,
    draggingRef: repoDraggingRef,
    inFlightRef: reorderInFlightRef,
    pendingRef: pendingReorderRef,
  });

  return {
    ...poll,
    // Exposed because opening and closing a project are order writes too, and
    // those live with the rest of the repository actions.
    orderWrites,
    draggingRepo,
    dragOverRepo,
    onRepoDragStart,
    onRepoDragMove,
    onRepoDragEnd,
  };
}
