// @vitest-environment happy-dom

import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ShortcutIntentProvider, useRegisterShortcutHandlers } from "../../hooks/shortcutIntents";
import { ShortcutLeaderProvider } from "../../hooks/shortcutLeader";
import { DEFAULT_LEADER, type ChordSpec } from "../../lib/leaderChord";
import { IDLE_LEADER, type LeaderState } from "../../lib/leaderState";
import type { HintClick } from "../../lib/shortcutHintBar";
import { ShortcutHintBar } from "./ShortcutHintBar";

afterEach(cleanup);

function Panel() {
  useRegisterShortcutHandlers({
    "terminal.newPane": () => undefined,
    "help.shortcuts": () => undefined,
  });
  return null;
}

function mount(state: LeaderState, leader: ChordSpec | null = DEFAULT_LEADER) {
  const onClick = vi.fn<(click: HintClick) => void>();
  render(
    <ShortcutIntentProvider>
      <ShortcutLeaderProvider leader={leader}>
        <Panel />
        <ShortcutHintBar state={state} onClick={onClick} />
      </ShortcutLeaderProvider>
    </ShortcutIntentProvider>,
  );
  const bar = document.querySelector<HTMLElement>('[role="toolbar"]')!;
  return { bar, onClick };
}

describe("ShortcutHintBar", () => {
  it("리더_키와_등록된_명령만_한_줄로_적는다", () => {
    const { bar } = mount(IDLE_LEADER);

    expect(bar.textContent).toContain("Ctrl+F: leader");
    expect(bar.textContent).toContain("Ctrl+F t: new pane");
    expect(bar.textContent).toContain("Ctrl+F ?: shortcuts");
    // 패널이 등록하지 않은 명령은 이 화면에서 할 수 없는 일이다.
    expect(bar.textContent).not.toContain("close pane");
    expect(bar.querySelector("[data-hint-chip]")).toBeNull();
  });

  it("리더가_눌린_상태에서는_PREFIX_칩이_켜진다", () => {
    const { bar } = mount({ armed: true, swapPending: false });

    expect(bar.querySelector("[data-hint-chip]")?.textContent).toBe("PREFIX");
    expect(bar.textContent).toContain("t: new pane");
    expect(bar.textContent).toContain("esc: cancel");
  });

  it("세그먼트를_누르면_클릭을_그대로_보고하고_키보드는_가져가지_않는다", () => {
    const { bar, onClick } = mount(IDLE_LEADER);
    const newPane = bar.querySelector<HTMLButtonElement>(
      '[data-hint-action="terminal.newPane"]',
    )!;

    const down = fireEvent.pointerDown(newPane);
    fireEvent.click(newPane);

    // pointerdown이 막혀야 버튼이 pane에서 caret을 빼앗지 않는다.
    expect(down).toBe(false);
    expect(onClick).toHaveBeenCalledWith({ kind: "run", action: "terminal.newPane" });

    fireEvent.click(bar.querySelector('[data-hint-action="arm"]')!);
    expect(onClick).toHaveBeenLastCalledWith({ kind: "arm" });
  });

  it("리더가_꺼져_있으면_그렇다고_적고_시트로_보낸다", () => {
    const { bar, onClick } = mount(IDLE_LEADER, null);

    expect(bar.textContent).toContain("leader: switched off");
    fireEvent.click(bar.querySelector('[data-hint-action="help.shortcuts"]')!);
    expect(onClick).toHaveBeenCalledWith({ kind: "run", action: "help.shortcuts" });
  });
});
