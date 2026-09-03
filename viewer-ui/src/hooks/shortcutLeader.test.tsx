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
    // ARIA has no two-step notation; a space-separated value would falsely
    // claim that the follow-up key runs the action by itself.
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
    // Deriving the value avoids stale bindings when the leader is rebound.
    const { button, chordButton, rerender } = mount(DEFAULT_LEADER);

    rerender(parseChord("Alt+G"));

    expect(button.getAttribute("title")).toBe("New terminal (Alt+G then t)");
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
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
    // Isolated component tests should not require this context.
    render(<Probe />);

    const button = screen.getByRole("button", { name: "new" });
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(button.getAttribute("title")).toBe("New terminal");
  });

  it("빈_aria_keyshortcuts를_렌더하지_않는다", () => {
    // An empty `aria-keyshortcuts` value is invalid even when the type allows it.
    mount(DEFAULT_LEADER);

    for (const node of document.querySelectorAll("[aria-keyshortcuts]")) {
      expect(node.getAttribute("aria-keyshortcuts")?.trim()).toBeTruthy();
    }
  });
});
