// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ShortcutLeaderProvider, useShortcutHint } from "./shortcutLeader";
import { DEFAULT_LEADER, parseChord, type ChordSpec } from "../lib/leaderChord";

// Vitest runs without globals, so RTL cannot auto-register its cleanup.
afterEach(cleanup);

/** Two controls: one bound by the leader plus a follow-up key, one bound by a
 *  standalone chord. The attribute is only for the second kind. */
function Probe() {
  const shortcut = useShortcutHint();
  return (
    <>
      <button {...shortcut("terminal.newPane", "New terminal")}>new</button>
      <button {...shortcut("project.next", "Next project")}>next</button>
    </>
  );
}

function mount(leader: ChordSpec | null) {
  const view = render(
    <ShortcutLeaderProvider leader={leader}>
      <Probe />
    </ShortcutLeaderProvider>,
  );
  return {
    button: screen.getByRole("button", { name: "new" }),
    chordButton: screen.getByRole("button", { name: "next" }),
    rerender: (next: ChordSpec | null) =>
      view.rerender(
        <ShortcutLeaderProvider leader={next}>
          <Probe />
        </ShortcutLeaderProvider>,
      ),
  };
}

describe("useShortcutHint", () => {
  it("leader_시퀀스는_title로만_말하고_aria_속성은_두지_않는다", () => {
    // ARIA에는 두 단계 시퀀스 표기가 없다. 공백으로 적으면 후속 키 하나로도
    // 실행된다는 틀린 주장이 되므로 속성을 아예 두지 않는다.
    const { button } = mount(DEFAULT_LEADER);

    expect(button.getAttribute("title")).toBe("New terminal (Ctrl+F then t)");
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
  });

  it("standalone_chord는_W3C_형태로_속성을_받는다", () => {
    const { chordButton } = mount(DEFAULT_LEADER);

    expect(chordButton.getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowRight",
    );
    expect(chordButton.getAttribute("title")).toBe(
      "Next project (Ctrl+Shift+ArrowRight)",
    );
  });

  it("리더를_바꾸면_같은_컨트롤이_새_키를_말한다", () => {
    // 컨트롤마다 문자열을 적어 두면 재바인딩이 일부만 반영된다.
    const { button, chordButton, rerender } = mount(DEFAULT_LEADER);

    rerender(parseChord("Alt+G"));

    expect(button.getAttribute("title")).toBe("New terminal (Alt+G then t)");
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
    // chord는 리더와 무관하므로 움직이지 않는다.
    expect(chordButton.getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowRight",
    );
  });

  it("리더가_꺼져_있으면_키를_주장하지_않는다", () => {
    const { button } = mount(null);

    expect(button.getAttribute("title")).toBe("New terminal");
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
  });

  it("provider_밖에서는_leader_키_없이_렌더된다", () => {
    // 격리된 컴포넌트 테스트가 이 context에 의존하지 않게 하는 쪽을 택했다.
    render(<Probe />);

    const button = screen.getByRole("button", { name: "new" });
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(button.getAttribute("title")).toBe("New terminal");
  });

  it("빈_aria_keyshortcuts를_렌더하지_않는다", () => {
    // `aria-keyshortcuts=""`는 그 자체로 버그다: 타입만 보면 놓친다.
    mount(DEFAULT_LEADER);

    for (const node of document.querySelectorAll("[aria-keyshortcuts]")) {
      expect(node.getAttribute("aria-keyshortcuts")?.trim()).toBeTruthy();
    }
  });
});
