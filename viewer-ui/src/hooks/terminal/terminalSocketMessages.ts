import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import {
  decodeTerminalControlFrame,
  decodeTerminalOutputFrame,
  type PaneSize,
  type TerminalServerMessage,
} from "../../api/terminal";
import type { LinkState } from "../../lib/terminal/attachStatus";
import { forgetPane, lastPaneOf, rememberPane } from "../../lib/terminal/lastPane";
import { reconcileOrder } from "../../lib/terminal/paneOrder";
import { applyRecovery, type RecoveryByPane } from "../../lib/terminal/recovery";
import type { PaneView } from "../../lib/terminal/terminalLayout";
import type {
  PendingTerminalWrite,
  TerminalReplayState,
} from "../../lib/terminal/terminalReplay";
import { queueReplayEnd } from "../../lib/terminal/terminalReplay";
import { writeTerminalData } from "../../lib/terminal/terminalWrites";
import { toast } from "../../lib/runtime/toast";
import { applySizeOwnerUpdate } from "../../lib/terminal/sizeActivity";

type Setter<T> = Dispatch<SetStateAction<T>>;

export interface TerminalMessageContext {
  repo: string;
  clientIdRef: MutableRefObject<number | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  pendingRef: MutableRefObject<Map<number, PendingTerminalWrite[]>>;
  replayRef: MutableRefObject<TerminalReplayState>;
  ptySizesRef: MutableRefObject<Map<number, PaneSize>>;
  askedSizesRef: MutableRefObject<Map<number, PaneSize>>;
  desiredSizesRef: MutableRefObject<Map<number, PaneSize>>;
  ownsSizeRef: MutableRefObject<boolean>;
  sizeOwnerGenerationRef: MutableRefObject<string | null>;
  onSizeAcquired: () => void;
  zoomAskedRef: MutableRefObject<number | null | undefined>;
  /** Not a plain setter: the socket hook remembers whether this page has ever
   *  been attached, so a link it loses reads as a reconnect. */
  setLink: (state: LinkState) => void;
  setPending: Setter<number | null>;
  setReplayLeft: Setter<number>;
  setPanes: Setter<number[]>;
  setActive: Setter<number | null>;
  setZoomed: Setter<number | null>;
  setTitles: Setter<Record<number, string>>;
  setOwnsSize: (owned: boolean) => void;
  setRecovery: Setter<RecoveryByPane>;
}

export function handleTerminalSocketMessage(
  data: unknown,
  context: TerminalMessageContext,
): void {
  if (typeof data === "string") {
    const message = decodeTerminalControlFrame(data);
    if (message) handleControlMessage(message, context);
    return;
  }
  if (!(data instanceof ArrayBuffer)) return;

  const frame = decodeTerminalOutputFrame(data);
  if (!frame) return;
  let replayToken: symbol | undefined;
  if (context.replayRef.current.active) {
    replayToken = context.replayRef.current.panes.get(frame.pane);
    if (!replayToken) {
      replayToken = Symbol();
      context.replayRef.current.panes.set(frame.pane, replayToken);
    }
  }
  const view = context.viewsRef.current.get(frame.pane);
  if (view) {
    if (replayToken) view.replayGate.begin();
    // Through the pane's backlog: bytes the parser has not drained yet are
    // counted, and a frame past the high-water mark waits instead of growing
    // xterm's queue without limit.
    writeTerminalData(view.term, frame.data);
    return;
  }
  const queue = context.pendingRef.current.get(frame.pane) ?? [];
  queue.push(frame.data);
  context.pendingRef.current.set(frame.pane, queue);
}

function handleControlMessage(
  message: TerminalServerMessage,
  context: TerminalMessageContext,
): void {
  switch (message.type) {
    case "hello":
      context.clientIdRef.current = message.client;
      context.setLink("live");
      context.replayRef.current.active = true;
      context.replayRef.current.panes.clear();
      context.replayRef.current.order.length = 0;
      context.replayRef.current.zoomed = false;
      context.setReplayLeft(message.panes);
      return;
    case "replay_complete":
      completeReplay(context);
      return;
    case "pending":
      context.setPending(message.count);
      return;
    case "created": {
      const pane = message.pane;
      if (context.replayRef.current.active) {
        context.replayRef.current.panes.set(pane, Symbol());
        const order = context.replayRef.current.order;
        if (!order.includes(pane)) order.push(pane);
      }
      context.ptySizesRef.current.set(pane, {
        rows: message.rows,
        cols: message.cols,
      });
      // A pane that was kept across a reconnect already has an emulator, and
      // the replay that follows was drawn against this grid. A pane the panel
      // opens now is resized from the same value when its view appears
      // (`useTerminalViews`); this is the other half of that promise. The
      // terminal is still empty here — the replay's screen comes next.
      const kept = context.viewsRef.current.get(pane);
      if (
        kept &&
        (kept.term.rows !== message.rows || kept.term.cols !== message.cols)
      ) {
        kept.term.resize(message.cols, message.rows);
      }
      const title = message.title;
      if (title) {
        context.setTitles((current) => ({
          ...current,
          [pane]: title,
        }));
      }
      // A reconnect keeps the panes on screen while the replay restores them,
      // so what was on the old connection must not be appended to a second
      // time. A pane that is already known stays where it is; the replay's
      // order is applied in full at its completion below.
      context.setPanes((current) =>
        current.includes(pane) ? current : [...current, pane],
      );
      context.setReplayLeft((left) => (left > 0 ? left - 1 : 0));
      if (
        message.client != null &&
        message.client === context.clientIdRef.current
      ) {
        context.setActive(pane);
        rememberPane(context.repo, pane);
      } else if (lastPaneOf(context.repo) === pane) {
        context.setActive(pane);
      }
      return;
    }
    case "exited":
      context.setPanes((current) =>
        current.filter((pane) => pane !== message.pane),
      );
      context.setActive((current) =>
        current === message.pane ? null : current,
      );
      context.pendingRef.current.delete(message.pane);
      context.replayRef.current.panes.delete(message.pane);
      context.ptySizesRef.current.delete(message.pane);
      context.askedSizesRef.current.delete(message.pane);
      context.desiredSizesRef.current.delete(message.pane);
      forgetPane(context.repo, message.pane);
      context.setTitles((current) => {
        if (!(message.pane in current)) return current;
        const next = { ...current };
        delete next[message.pane];
        return next;
      });
      return;
    case "resized":
      context.ptySizesRef.current.set(message.pane, {
        rows: message.rows,
        cols: message.cols,
      });
      context.viewsRef.current
        .get(message.pane)
        ?.term.resize(message.cols, message.rows);
      return;
    case "recovery":
      context.setRecovery((current) => applyRecovery(current, message));
      return;
    case "size_owner":
      // The synchronous ref is the activity handler's ownership snapshot. Clear
      // old fit requests only on a real acquisition, not on same-owner acks.
      applySizeOwnerUpdate(
        context.ownsSizeRef,
        context.sizeOwnerGenerationRef,
        message.owned,
        message.generation,
        () => {
          context.askedSizesRef.current.clear();
          context.onSizeAcquired();
        },
      );
      context.setOwnsSize(message.owned);
      return;
    case "reordered":
      context.setPanes((current) => reconcileOrder(current, message.order));
      return;
    case "zoomed":
      context.replayRef.current.zoomed = true;
      context.zoomAskedRef.current = undefined;
      context.setZoomed(message.pane ?? null);
      return;
    case "error":
      toast.error(message.message);
      return;
    // A pane asking for its person (`nightcrow notify`, or a plugin). Said
    // here for whoever is looking at the page; a closed page hears it through
    // Web Push instead.
    case "attention":
      toast.info(message.message ?? "A terminal wants your attention");
      return;
  }
  const unhandled: never = message;
  return unhandled;
}

function completeReplay(context: TerminalMessageContext): void {
  const replay = context.replayRef.current;
  if (!replay.active) return;
  replay.active = false;
  context.setReplayLeft(0);
  for (const [pane, token] of replay.panes) {
    const view = context.viewsRef.current.get(pane);
    if (!view) {
      const queue = context.pendingRef.current.get(pane) ?? [];
      queue.push({ kind: "replay_complete", token });
      context.pendingRef.current.set(pane, queue);
      continue;
    }
    queueReplayEnd(view.term, view.replayGate, () => {
      if (
        context.viewsRef.current.get(pane) === view &&
        replay.panes.get(pane) === token
      ) {
        replay.panes.delete(pane);
      }
    });
  }
  // The replay's `created` frames are the whole truth for this connection, in
  // their order: a reconnect may have outlived a pane, gained one, or had the
  // panes reordered by another screen. Replacing the list here also drops the
  // cells of panes that are gone (with their emulators) before any live frame
  // can name them.
  const replayed = replay.order.slice();
  context.setPanes((current) =>
    current.length === replayed.length &&
    current.every((pane, index) => pane === replayed[index])
      ? current
      : replayed,
  );
  // A page that reconnects keeps its zoom through the gap; if the replay did
  // not announce one, the pane it named is gone and the grid is current.
  if (!replay.zoomed) context.setZoomed(null);
  context.setActive((current) =>
    current !== null && replayed.includes(current) ? current : null,
  );
  context.setTitles((current) => {
    const stale = Object.keys(current).some((pane) => !replayed.includes(Number(pane)));
    if (!stale) return current;
    const next: Record<number, string> = {};
    for (const pane of replayed) {
      const title = current[pane];
      if (title !== undefined) next[pane] = title;
    }
    return next;
  });
}
