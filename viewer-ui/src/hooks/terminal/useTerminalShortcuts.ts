import { useEffect, useMemo, useRef } from "react";
import type { MutableRefObject } from "react";
import { sendTerminalMessage } from "../../api/terminal";
import type { LinkState } from "../../lib/attachStatus";
import { paneAt, swapOrder } from "../../lib/paneOrder";
import { SHORTCUT_ACTIONS, focusPaneNumber } from "../../lib/shortcutActions";
import {
  useRegisterShortcutHandlers,
  useShortcutIntents,
  type ShortcutHandlers,
} from "../shortcutIntents";

// The panel's half of the shortcut registry.
//
// These commands cannot be bound at the page level: each needs the socket, the
// pane list or an xterm instance, none of which leave this component. So the
// panel publishes them on the intent bus and the one keyboard handler calls
// them, which is also what makes them unavailable — to the keyboard and to the
// help sheet alike — the moment there is no panel or no pane to act on. The
// registration below is conditional for exactly that reason.

export interface TerminalPaneCommands {
  create: () => void;
  closePane: (pane: number) => void;
  claimSize: () => void;
  reorder: (order: number[]) => void;
  toggleZoom: (pane: number) => void;
}

export interface UseTerminalShortcutsArgs {
  socketRef: MutableRefObject<WebSocket | null>;
  panes: number[];
  active: number | null;
  /** Where the socket is. Anything but `live` disarms an armed leader. */
  link: LinkState;
  commands: TerminalPaneCommands;
  focusPane: (pane: number) => void;
  cancelRecovery: (pane: number) => void;
}

export function useTerminalShortcuts({
  socketRef,
  panes,
  active,
  link,
  commands,
  focusPane,
  cancelRecovery,
}: UseTerminalShortcutsArgs): void {
  const intents = useShortcutIntents();
  // The panel rebuilds these callbacks every render. Read through a ref so the
  // registration below depends on the pane list alone: re-registering on every
  // render would churn the bus for a set of handlers that has not changed.
  const live = useRef({ commands, focusPane, cancelRecovery, socketRef });
  live.current = { commands, focusPane, cancelRecovery, socketRef };

  const handlers = useMemo<ShortcutHandlers>(() => {
    const map: ShortcutHandlers = {
      "terminal.newPane": () => live.current.commands.create(),
    };
    // The digit row's numbering belongs to the registry, so it is read from
    // there rather than counted again here.
    for (const action of SHORTCUT_ACTIONS) {
      const nth = focusPaneNumber(action.id);
      if (nth === null) continue;
      const pane = paneAt(panes, nth);
      if (pane === null) continue;
      map[action.id] = () => live.current.focusPane(pane);
    }
    if (active === null) return map;
    map["terminal.closePane"] = () => live.current.commands.closePane(active);
    map["terminal.claimSizing"] = () => live.current.commands.claimSize();
    map["terminal.cancelRecovery"] = () => live.current.cancelRecovery(active);
    // Runs nothing by design: `reduceLeader` holds the leader armed for the pane
    // digit and never emits this action. The registration is what states the
    // command is available — there is a pane to swap — so the keyboard and the
    // help sheet cannot disagree about it.
    map["terminal.swapPanePrompt"] = noop;
    map.swapPanes = (nth: number) => {
      const target = paneAt(panes, nth);
      if (target === null) return;
      live.current.commands.reorder(swapOrder(panes, active, target));
    };
    map.zoomActivePane = () => live.current.commands.toggleZoom(active);
    // The same message the on-screen key bar sends, over the same socket. A
    // second path would be a second thing to keep in step with the protocol.
    map.sendInput = (data: string) =>
      void sendTerminalMessage(live.current.socketRef.current, {
        type: "input",
        pane: active,
        data,
      });
    return map;
  }, [panes, active]);

  useRegisterShortcutHandlers(handlers);

  // A reconnect rebuilds every pane and hands out ids that mean something else,
  // so a leader armed before it must not spend its follow-up on the new ones.
  // Reported from here because the page cannot see the socket.
  const disarm = intents?.disarm;
  useEffect(() => {
    if (link === "live") return;
    disarm?.();
  }, [link, disarm]);
}

function noop(): void {}
