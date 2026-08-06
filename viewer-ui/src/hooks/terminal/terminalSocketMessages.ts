import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import {
  decodeTerminalControlFrame,
  decodeTerminalOutputFrame,
  type PaneSize,
  type TerminalServerMessage,
} from "../../api/terminal";
import type { LinkState } from "../../lib/attachStatus";
import { reconcileOrder } from "../../lib/paneOrder";
import { applyRecovery, type RecoveryByPane } from "../../lib/recovery";
import type { PaneView } from "../../lib/terminalLayout";
import { toast } from "../../lib/toast";

type Setter<T> = Dispatch<SetStateAction<T>>;

export interface TerminalMessageContext {
  repo: string;
  clientIdRef: MutableRefObject<number | null>;
  viewsRef: MutableRefObject<Map<number, PaneView>>;
  pendingRef: MutableRefObject<Map<number, Uint8Array[]>>;
  sentSizesRef: MutableRefObject<Map<number, PaneSize>>;
  lastActiveByRepoRef: MutableRefObject<Map<string, number>>;
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
  setOwnsSize: Setter<boolean>;
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
  const view = context.viewsRef.current.get(frame.pane);
  if (view) {
    view.term.write(frame.data);
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
      context.setReplayLeft(message.panes);
      return;
    case "pending":
      context.setPending(message.count);
      return;
    case "created": {
      const pane = message.pane;
      context.sentSizesRef.current.set(pane, {
        rows: message.rows,
        cols: message.cols,
      });
      const title = message.title;
      if (title) {
        context.setTitles((current) => ({
          ...current,
          [pane]: title,
        }));
      }
      context.setPanes((current) => [...current, pane]);
      context.setReplayLeft((left) => (left > 0 ? left - 1 : 0));
      if (
        message.client != null &&
        message.client === context.clientIdRef.current
      ) {
        context.setActive(pane);
        context.lastActiveByRepoRef.current.set(context.repo, pane);
      } else if (context.lastActiveByRepoRef.current.get(context.repo) === pane) {
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
      context.sentSizesRef.current.delete(message.pane);
      context.setTitles((current) => {
        if (!(message.pane in current)) return current;
        const next = { ...current };
        delete next[message.pane];
        return next;
      });
      return;
    case "resized":
      context.sentSizesRef.current.set(message.pane, {
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
      context.setOwnsSize(message.owned);
      return;
    case "reordered":
      context.setPanes((current) => reconcileOrder(current, message.order));
      return;
    case "zoomed":
      context.zoomAskedRef.current = undefined;
      context.setZoomed(message.pane ?? null);
      return;
    case "error":
      toast.error(message.message);
      return;
  }
  const unhandled: never = message;
  return unhandled;
}
