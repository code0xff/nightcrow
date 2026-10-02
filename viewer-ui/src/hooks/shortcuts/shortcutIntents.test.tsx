// @vitest-environment happy-dom

import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ShortcutIntentProvider,
  useShortcutAvailability,
  useShortcutIntents,
  type ShortcutIntents,
} from "./shortcutIntents";

afterEach(cleanup);

/** The bus as a component sees it. */
function bus(): ShortcutIntents {
  const held: { current: ShortcutIntents | null } = { current: null };
  function Probe() {
    held.current = useShortcutIntents();
    return null;
  }
  render(
    <ShortcutIntentProvider>
      <Probe />
    </ShortcutIntentProvider>,
  );
  if (!held.current) throw new Error("no bus");
  return held.current;
}

/** Registration bumps the availability version, which is React state. */
function register(
  intents: ShortcutIntents,
  handlers: Parameters<ShortcutIntents["registerShortcutHandlers"]>[0],
) {
  let off = () => {};
  act(() => {
    off = intents.registerShortcutHandlers(handlers);
  });
  return () => act(() => off());
}

describe("shortcutIntents 인텐트 버스", () => {
  it("등록한_핸들러를_id로_실행한다", () => {
    const intents = bus();
    const newPane = vi.fn();
    register(intents, { "terminal.newPane": newPane });

    expect(intents.runAction("terminal.newPane")).toBe(true);
    expect(newPane).toHaveBeenCalledTimes(1);
  });

  it("아무도_등록하지_않은_명령은_돌지_않는다고_말한다", () => {
    const intents = bus();

    expect(intents.runAction("terminal.newPane")).toBe(false);
    expect(intents.isAvailable("terminal.newPane")).toBe(false);
  });

  it("나중_등록이_같은_id를_가져간다", () => {
    const intents = bus();
    const first = vi.fn();
    const second = vi.fn();
    register(intents, { "terminal.newPane": first });
    register(intents, { "terminal.newPane": second });

    intents.runAction("terminal.newPane");

    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("해제는_가려졌던_핸들러를_되살리지_않는다", () => {
    // There is one terminal panel: a shadowed handler is a mistake, not a stack
    // to pop back to.
    const intents = bus();
    const first = vi.fn();
    register(intents, { "terminal.newPane": first });
    const off = register(intents, { "terminal.newPane": vi.fn() });

    off();

    expect(intents.isAvailable("terminal.newPane")).toBe(false);
    expect(first).not.toHaveBeenCalled();
  });

  it("해제는_남이_대체한_핸들러를_지우지_않는다", () => {
    const intents = bus();
    const off = register(intents, { "terminal.newPane": vi.fn() });
    const later = vi.fn();
    register(intents, { "terminal.newPane": later });

    off();

    expect(intents.runAction("terminal.newPane")).toBe(true);
    expect(later).toHaveBeenCalledTimes(1);
  });

  it("터미널이_주는_보조_경로는_등록_여부를_그대로_보고한다", () => {
    const intents = bus();

    expect(intents.swapPanes(2)).toBe(false);
    expect(intents.zoomActivePane()).toBe(false);
    // Nothing registered: sending must be a no-op rather than a throw.
    intents.sendLiteralLeader("\x06");

    const sendInput = vi.fn();
    const swapPanes = vi.fn();
    const zoomActivePane = vi.fn();
    register(intents, { sendInput, swapPanes, zoomActivePane });

    intents.sendLiteralLeader("\x06");
    expect(intents.swapPanes(2)).toBe(true);
    expect(intents.zoomActivePane()).toBe(true);

    expect(sendInput).toHaveBeenCalledWith("\x06");
    expect(swapPanes).toHaveBeenCalledWith(2);
    expect(zoomActivePane).toHaveBeenCalledTimes(1);
  });

  it("해제를_보고받은_구독자만_불린다", () => {
    const intents = bus();
    const listener = vi.fn();
    const off = intents.onDisarm(listener);

    intents.disarm();
    off();
    intents.disarm();

    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("가용성을_읽는_컴포넌트는_등록에_맞춰_다시_그려진다", () => {
    // What the help sheet depends on: the bus keeps one identity so a
    // registration cannot re-trigger the effect that made it, and availability
    // travels on its own so the sheet still sees the change.
    const held: { current: ShortcutIntents | null } = { current: null };
    function Sheet() {
      const available = useShortcutAvailability();
      held.current = useShortcutIntents();
      return <span>{available("terminal.newPane") ? "on" : "off"}</span>;
    }
    render(
      <ShortcutIntentProvider>
        <Sheet />
      </ShortcutIntentProvider>,
    );
    expect(screen.getByText("off")).toBeTruthy();

    const identity = held.current;
    const off = register(held.current!, { "terminal.newPane": vi.fn() });
    expect(screen.getByText("on")).toBeTruthy();
    expect(held.current).toBe(identity);

    off();
    expect(screen.getByText("off")).toBeTruthy();
  });

  it("프로바이더_밖에서는_버스가_없다고_말한다", () => {
    const held: { current: ShortcutIntents | null } = { current: null };
    function Probe() {
      held.current = useShortcutIntents();
      return null;
    }
    render(<Probe />);

    expect(held.current).toBeNull();
  });
});
