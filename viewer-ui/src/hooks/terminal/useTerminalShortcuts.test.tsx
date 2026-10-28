// @vitest-environment happy-dom
//
// The panel's half of the registry, checked against the socket rather than
// against internals: a claimed leader sequence must run one command and put
// nothing on the wire, and the leader pressed twice must put exactly one `input`
// frame there.

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_LEADER } from "../../lib/shortcuts/leaderChord";
import {
  ShortcutIntentProvider,
  useShortcutIntents,
  type ShortcutIntents,
} from "../shortcuts/shortcutIntents";
import { ShortcutEngine, leader, pane, press } from "../shortcuts/useShortcuts.harness";
import {
  useTerminalShortcuts,
  type UseTerminalShortcutsArgs,
} from "./useTerminalShortcuts";

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

function Panel({
  args,
  bus,
}: {
  args: UseTerminalShortcutsArgs;
  bus: { current: ShortcutIntents | null };
}) {
  bus.current = useShortcutIntents();
  useTerminalShortcuts(args);
  return null;
}

/** The panel with three panes, the middle one active — so an ordinal digit and
 *  a pane id are never the same number by accident. */
function mount(over: Partial<UseTerminalShortcutsArgs> = {}) {
  const commands = {
    create: vi.fn(),
    closePane: vi.fn(),
    claimSize: vi.fn(),
    reorder: vi.fn(),
    toggleZoom: vi.fn(),
  };
  const focusPane = vi.fn();
  const cancelRecovery = vi.fn();
  const openCompose = vi.fn();
  const send = vi.fn();
  const socketRef = {
    current: { readyState: WebSocket.OPEN, send } as unknown as WebSocket,
  };
  const bus: { current: ShortcutIntents | null } = { current: null };
  const args: UseTerminalShortcutsArgs = {
    socketRef,
    panes: [7, 8, 9],
    active: 8,
    zoom: null,
    link: "live",
    commands,
    focusPane,
    cancelRecovery,
    openCompose,
    ...over,
  };
  const tree = (current: UseTerminalShortcutsArgs) => (
    <ShortcutIntentProvider>
      <ShortcutEngine
        args={{
          enabled: true,
          leader: DEFAULT_LEADER,
          dialogOpen: false,
          repo: "r1",
        }}
      />
      <Panel args={current} bus={bus} />
    </ShortcutIntentProvider>
  );
  const view = render(tree(args));
  return {
    commands,
    focusPane,
    cancelRecovery,
    openCompose,
    send,
    bus,
    update: (next: Partial<UseTerminalShortcutsArgs>) =>
      view.rerender(tree({ ...args, ...next })),
  };
}

/** The `input` frames the panel put on the wire. */
function inputs(send: ReturnType<typeof vi.fn>): unknown[] {
  return send.mock.calls
    .map(([raw]) => JSON.parse(raw as string) as { type: string })
    .filter((message) => message.type === "input");
}

describe("useTerminalShortcuts panel commands", () => {
  it("leader commands call controls already used by the panel", () => {
    const { commands, cancelRecovery, openCompose } = mount();

    for (const [key, check] of [
      ["t", () => expect(commands.create).toHaveBeenCalledTimes(1)],
      ["w", () => expect(commands.closePane).toHaveBeenCalledWith(8)],
      ["z", () => expect(commands.claimSize).toHaveBeenCalledTimes(1)],
      ["c", () => expect(cancelRecovery).toHaveBeenCalledWith(8)],
      ["m", () => expect(openCompose).toHaveBeenCalledTimes(1)],
    ] as const) {
      leader();
      press(document.body, { key });
      check();
    }
  });

  it("a command sequence sends no bytes over the socket", () => {
    const { xterm, xtermKeydown } = pane();
    const { commands, send } = mount();

    // Typed with the keyboard in a pane, which is where the leader is used.
    leader(xterm);
    press(xterm, { key: "t" });

    expect(commands.create).toHaveBeenCalledTimes(1);
    expect(send).not.toHaveBeenCalled();
    expect(xtermKeydown).not.toHaveBeenCalled();
  });

  it("pressing the leader twice sends only one input frame", () => {
    const { send } = mount();

    leader();
    leader();

    expect(inputs(send)).toEqual([
      { type: "input", pane: 8, data: "\x06" },
    ]);
  });

  it("pane numbers use visible order rather than ids", () => {
    const { focusPane } = mount();

    leader();
    press(document.body, { key: "4" });

    // `4` is the second pane in the digit row, which is pane id 8 here.
    expect(focusPane).toHaveBeenCalledWith(8);
  });

  it("swap exchanges two positions using the existing reorder", () => {
    const { commands } = mount();

    leader();
    press(document.body, { key: "s" });
    press(document.body, { key: "3" });

    // The active pane (8) and the first pane (7) exchange places; nothing else
    // in the order moves.
    expect(commands.reorder).toHaveBeenCalledWith([8, 7, 9]);
  });

  it("a swap targeting a missing pane does not change the layout", () => {
    const { commands } = mount();

    leader();
    press(document.body, { key: "s" });
    press(document.body, { key: "0" });

    expect(commands.reorder).not.toHaveBeenCalled();
  });

  it("zoom-half zooms the active pane", () => {
    const { commands, bus } = mount();

    expect(bus.current?.zoomActivePane()).toBe(true);

    expect(commands.toggleZoom).toHaveBeenCalledWith(8);
  });

  it("pane-dependent commands are unavailable when there is no active pane", () => {
    const { bus } = mount({ panes: [], active: null });
    const available = bus.current!.isAvailable;

    for (const id of [
      "terminal.closePane",
      "terminal.claimSizing",
      "terminal.cancelRecovery",
      "terminal.composeMessage",
      "terminal.swapPanePrompt",
      "focus.pane1",
    ] as const) {
      expect(available(id), id).toBe(false);
    }
    // A panel with no panes is exactly where opening one has to be reachable.
    expect(available("terminal.newPane")).toBe(true);
  });

  it("availability includes only panes that actually exist", () => {
    const { bus } = mount();
    const available = bus.current!.isAvailable;

    expect(available("focus.pane3")).toBe(true);
    expect(available("focus.pane4")).toBe(false);
    expect(available("terminal.swapPanePrompt")).toBe(true);
  });

  it("commands disappear when the panel disappears", () => {
    const { bus } = mount();
    const intents = bus.current!;
    expect(intents.isAvailable("terminal.newPane")).toBe(true);

    cleanup();

    expect(intents.isAvailable("terminal.newPane")).toBe(false);
    expect(intents.runAction("terminal.newPane")).toBe(false);
  });

  it("disarms the leader while the socket reconnects", () => {
    const { commands, update } = mount();

    leader();
    update({ link: "reconnecting" });
    const after = press(document.body, { key: "t" });

    expect(after.defaultPrevented).toBe(false);
    expect(commands.create).not.toHaveBeenCalled();
  });
});

describe("useTerminalShortcuts panel side of the focus ring", () => {
  it("the cursor reports the active pane index and pane count", () => {
    const { bus } = mount();

    expect(bus.current?.paneCursor()).toEqual({ index: 1, count: 3 });
  });

  it("the cursor index is -1 when there is no active pane", () => {
    const { bus } = mount({ active: null });

    expect(bus.current?.paneCursor()).toEqual({ index: -1, count: 3 });
  });

  it("in the grid, focuses the pane selected by position", () => {
    const { bus, focusPane, commands } = mount();

    expect(bus.current?.focusPaneAt(2)).toBe(true);

    expect(focusPane).toHaveBeenCalledWith(9);
    expect(commands.toggleZoom).not.toHaveBeenCalled();
  });

  it("moves zoom instead of focus while zoomed", () => {
    // `usePaneFocus` would put the keyboard straight back on the zoomed pane,
    // so what changes is which pane fills the panel; the echo moves the keyboard.
    const { bus, focusPane, commands } = mount({ zoom: 8 });

    bus.current?.focusPaneAt(2);

    expect(commands.toggleZoom).toHaveBeenCalledWith(9);
    expect(focusPane).not.toHaveBeenCalled();
  });

  it("does not change zoom when the selected pane is already zoomed", () => {
    const { bus, focusPane, commands } = mount({ zoom: 8 });

    bus.current?.focusPaneAt(1);

    expect(focusPane).toHaveBeenCalledWith(8);
    expect(commands.toggleZoom).not.toHaveBeenCalled();
  });

  it("does nothing for an out-of-range index", () => {
    const { bus, focusPane } = mount();

    bus.current?.focusPaneAt(5);

    expect(focusPane).not.toHaveBeenCalled();
  });

  it("focusPaneAt is unavailable when there are no panes", () => {
    const { bus } = mount({ panes: [], active: null });

    expect(bus.current?.focusPaneAt(0)).toBe(false);
    expect(bus.current?.paneCursor()).toEqual({ index: -1, count: 0 });
  });
});
