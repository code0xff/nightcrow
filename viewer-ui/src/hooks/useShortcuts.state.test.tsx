// @vitest-environment happy-dom

import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { DEFAULT_LEADER } from "../lib/leaderChord";
import type { LeaderState } from "../lib/leaderState";
import { ShortcutIntentProvider } from "./shortcutIntents";
import { useShortcuts, type ShortcutEngine } from "./useShortcuts";
import { leader, press } from "./useShortcuts.harness";

afterEach(cleanup);

function Probe({ held }: { held: { current: ShortcutEngine | null } }) {
  held.current = useShortcuts({
    enabled: true,
    leader: DEFAULT_LEADER,
    dialogOpen: false,
    repo: "r1",
  });
  return null;
}

function mount() {
  const held: { current: ShortcutEngine | null } = { current: null };
  render(
    <ShortcutIntentProvider>
      <Probe held={held} />
    </ShortcutIntentProvider>,
  );
  return {
    state: () => held.current!.state,
    arm: () => act(() => held.current!.arm()),
    disarm: () => act(() => held.current!.disarm()),
  };
}

const ARMED: LeaderState = { armed: true, swapPending: false };

describe("useShortcuts state", () => {
  it("리더를_누르면_보이는_상태가_armed로_바뀌고_esc로_돌아온다", () => {
    const engine = mount();
    expect(engine.state()).toEqual({ armed: false });

    act(() => void leader());
    expect(engine.state()).toEqual(ARMED);

    act(() => void press(document.body, { key: "Escape" }));
    expect(engine.state()).toEqual({ armed: false });
  });

  it("힌트_줄의_클릭은_같은_리듀서를_지나_상태를_바꾼다", () => {
    const engine = mount();

    engine.arm();
    expect(engine.state()).toEqual(ARMED);

    // 무장된 리더가 힌트 클릭으로 명령을 실행했으면 다음 키가 후속 키로 읽히지 않아야 한다.
    engine.disarm();
    expect(engine.state()).toEqual({ armed: false });
  });
});
