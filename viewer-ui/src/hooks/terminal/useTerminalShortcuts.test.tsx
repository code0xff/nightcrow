// @vitest-environment happy-dom
//
// The panel's half of the registry, checked against the socket rather than
// against internals: a claimed leader sequence must run one command and put
// nothing on the wire, and the leader pressed twice must put exactly one `input`
// frame there.

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DEFAULT_LEADER } from "../../lib/leaderChord";
import {
  ShortcutIntentProvider,
  useShortcutIntents,
  type ShortcutIntents,
} from "../shortcutIntents";
import { ShortcutEngine, leader, pane, press } from "../useShortcuts.harness";
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

describe("useTerminalShortcuts 패널 명령", () => {
  it("리더_명령은_패널이_이미_쓰는_컨트롤을_부른다", () => {
    const { commands, cancelRecovery } = mount();

    for (const [key, check] of [
      ["t", () => expect(commands.create).toHaveBeenCalledTimes(1)],
      ["w", () => expect(commands.closePane).toHaveBeenCalledWith(8)],
      ["z", () => expect(commands.claimSize).toHaveBeenCalledTimes(1)],
      ["c", () => expect(cancelRecovery).toHaveBeenCalledWith(8)],
    ] as const) {
      leader();
      press(document.body, { key });
      check();
    }
  });

  it("명령_시퀀스는_소켓에_한_바이트도_쓰지_않는다", () => {
    const { xterm, xtermKeydown } = pane();
    const { commands, send } = mount();

    // Typed with the keyboard in a pane, which is where the leader is used.
    leader(xterm);
    press(xterm, { key: "t" });

    expect(commands.create).toHaveBeenCalledTimes(1);
    expect(send).not.toHaveBeenCalled();
    expect(xtermKeydown).not.toHaveBeenCalled();
  });

  it("리더를_두_번_누르면_input_프레임_하나만_나간다", () => {
    const { send } = mount();

    leader();
    leader();

    expect(inputs(send)).toEqual([
      { type: "input", pane: 8, data: "\x06" },
    ]);
  });

  it("pane_숫자는_id가_아니라_보이는_순서다", () => {
    const { focusPane } = mount();

    leader();
    press(document.body, { key: "4" });

    // `4` is the second pane in the digit row, which is pane id 8 here.
    expect(focusPane).toHaveBeenCalledWith(8);
  });

  it("스왑은_기존_reorder로_두_자리를_바꾼다", () => {
    const { commands } = mount();

    leader();
    press(document.body, { key: "s" });
    press(document.body, { key: "3" });

    // The active pane (8) and the first pane (7) exchange places; nothing else
    // in the order moves.
    expect(commands.reorder).toHaveBeenCalledWith([8, 7, 9]);
  });

  it("없는_pane을_가리키는_스왑은_배치를_건드리지_않는다", () => {
    const { commands } = mount();

    leader();
    press(document.body, { key: "s" });
    press(document.body, { key: "0" });

    expect(commands.reorder).not.toHaveBeenCalled();
  });

  it("줌_절반은_활성_pane을_줌한다", () => {
    const { commands, bus } = mount();

    expect(bus.current?.zoomActivePane()).toBe(true);

    expect(commands.toggleZoom).toHaveBeenCalledWith(8);
  });

  it("활성_pane이_없으면_pane에_기대는_명령은_가용하지_않다", () => {
    const { bus } = mount({ panes: [], active: null });
    const available = bus.current!.isAvailable;

    for (const id of [
      "terminal.closePane",
      "terminal.claimSizing",
      "terminal.cancelRecovery",
      "terminal.swapPanePrompt",
      "focus.pane1",
    ] as const) {
      expect(available(id), id).toBe(false);
    }
    // A panel with no panes is exactly where opening one has to be reachable.
    expect(available("terminal.newPane")).toBe(true);
  });

  it("가용성은_실제로_있는_pane까지만_말한다", () => {
    const { bus } = mount();
    const available = bus.current!.isAvailable;

    expect(available("focus.pane3")).toBe(true);
    expect(available("focus.pane4")).toBe(false);
    expect(available("terminal.swapPanePrompt")).toBe(true);
  });

  it("패널이_사라지면_명령도_사라진다", () => {
    const { bus } = mount();
    const intents = bus.current!;
    expect(intents.isAvailable("terminal.newPane")).toBe(true);

    cleanup();

    expect(intents.isAvailable("terminal.newPane")).toBe(false);
    expect(intents.runAction("terminal.newPane")).toBe(false);
  });

  it("소켓이_다시_붙는_동안_리더는_해제된다", () => {
    const { commands, update } = mount();

    leader();
    update({ link: "reconnecting" });
    const after = press(document.body, { key: "t" });

    expect(after.defaultPrevented).toBe(false);
    expect(commands.create).not.toHaveBeenCalled();
  });
});

describe("useTerminalShortcuts 포커스 링의 패널 쪽", () => {
  it("커서는_활성_pane의_순서와_pane_수를_말한다", () => {
    const { bus } = mount();

    expect(bus.current?.paneCursor()).toEqual({ index: 1, count: 3 });
  });

  it("활성_pane이_없으면_커서_index는_-1이다", () => {
    const { bus } = mount({ active: null });

    expect(bus.current?.paneCursor()).toEqual({ index: -1, count: 3 });
  });

  it("그리드에서는_순서로_고른_pane에_포커스한다", () => {
    const { bus, focusPane, commands } = mount();

    expect(bus.current?.focusPaneAt(2)).toBe(true);

    expect(focusPane).toHaveBeenCalledWith(9);
    expect(commands.toggleZoom).not.toHaveBeenCalled();
  });

  it("zoom_중에는_포커스_대신_zoom을_옮긴다", () => {
    // `usePaneFocus` would put the keyboard straight back on the zoomed pane,
    // so what changes is which pane fills the panel; the echo moves the keyboard.
    const { bus, focusPane, commands } = mount({ zoom: 8 });

    bus.current?.focusPaneAt(2);

    expect(commands.toggleZoom).toHaveBeenCalledWith(9);
    expect(focusPane).not.toHaveBeenCalled();
  });

  it("zoom된_pane_자체를_고르면_zoom을_건드리지_않는다", () => {
    const { bus, focusPane, commands } = mount({ zoom: 8 });

    bus.current?.focusPaneAt(1);

    expect(focusPane).toHaveBeenCalledWith(8);
    expect(commands.toggleZoom).not.toHaveBeenCalled();
  });

  it("범위_밖_순서는_아무것도_하지_않는다", () => {
    const { bus, focusPane } = mount();

    bus.current?.focusPaneAt(5);

    expect(focusPane).not.toHaveBeenCalled();
  });

  it("pane이_없으면_focusPaneAt은_가용하지_않다", () => {
    const { bus } = mount({ panes: [], active: null });

    expect(bus.current?.focusPaneAt(0)).toBe(false);
    expect(bus.current?.paneCursor()).toEqual({ index: -1, count: 0 });
  });
});
