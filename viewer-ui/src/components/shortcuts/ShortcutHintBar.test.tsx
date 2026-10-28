// @vitest-environment happy-dom

import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ShortcutIntentProvider, useRegisterShortcutHandlers } from "../../hooks/shortcuts/shortcutIntents";
import { ShortcutLeaderProvider } from "../../hooks/shortcuts/shortcutLeader";
import { DEFAULT_LEADER, type ChordSpec } from "../../lib/shortcuts/leaderChord";
import { IDLE_LEADER, type LeaderState } from "../../lib/shortcuts/leaderState";
import type { HintClick } from "../../lib/shortcuts/shortcutHintBar";
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
  it("shows the leader key and registered commands on one row", () => {
    const { bar } = mount(IDLE_LEADER);

    expect(bar.textContent).toContain("Ctrl+F: leader");
    expect(bar.textContent).toContain("Ctrl+F t: new pane");
    expect(bar.textContent).toContain("Ctrl+F ?: shortcuts");
    expect(bar.textContent).not.toContain("close pane");
    expect(bar.querySelector("[data-hint-chip]")).toBeNull();
  });

  it("the PREFIX chip is active while the leader is pressed", () => {
    const { bar } = mount({ armed: true, swapPending: false });

    expect(bar.querySelector("[data-hint-chip]")?.textContent).toBe("PREFIX");
    expect(bar.textContent).toContain("t: new pane");
    expect(bar.textContent).toContain("esc: cancel");
  });

  it("clicking a segment reports the click and does not take keyboard focus", () => {
    const { bar, onClick } = mount(IDLE_LEADER);
    const newPane = bar.querySelector<HTMLButtonElement>(
      '[data-hint-action="terminal.newPane"]',
    )!;

    const down = fireEvent.pointerDown(newPane);
    fireEvent.click(newPane);

    // Blocking pointerdown keeps the button from stealing the pane's caret.
    expect(down).toBe(false);
    expect(onClick).toHaveBeenCalledWith({ kind: "run", action: "terminal.newPane" });

    fireEvent.click(bar.querySelector('[data-hint-action="arm"]')!);
    expect(onClick).toHaveBeenLastCalledWith({ kind: "arm" });
  });

  it("when the leader is disabled, says so and links to the sheet", () => {
    const { bar, onClick } = mount(IDLE_LEADER, null);

    expect(bar.textContent).toContain("leader: switched off");
    fireEvent.click(bar.querySelector('[data-hint-action="help.shortcuts"]')!);
    expect(onClick).toHaveBeenCalledWith({ kind: "run", action: "help.shortcuts" });
  });
});
