// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ShortcutLeaderProvider, useShortcutHint } from "./shortcutLeader";
import { DEFAULT_LEADER, parseChord, type ChordSpec } from "../../lib/shortcuts/leaderChord";

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
  it("leader sequences use title only and set no aria attribute", () => {
    // ARIA has no two-step notation; a space-separated value would falsely
    // claim that the follow-up key runs the action by itself.
    const { button } = mount(DEFAULT_LEADER);

    expect(button.getAttribute("title")).toBe("New terminal (Ctrl+F then t)");
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
  });

  it("standalone chords use the W3C attribute format", () => {
    const { chordButton } = mount(DEFAULT_LEADER);

    expect(chordButton.getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowRight",
    );
    expect(chordButton.getAttribute("title")).toBe(
      "Next project (Ctrl+Shift+ArrowRight)",
    );
  });

  it("changing the leader makes the same control expose the new key", () => {
    // Deriving the value avoids stale bindings when the leader is rebound.
    const { button, chordButton, rerender } = mount(DEFAULT_LEADER);

    rerender(parseChord("Alt+G"));

    expect(button.getAttribute("title")).toBe("New terminal (Alt+G then t)");
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(chordButton.getAttribute("aria-keyshortcuts")).toBe(
      "Control+Shift+ArrowRight",
    );
  });

  it("does not claim a key when the leader is disabled", () => {
    const { button } = mount(null);

    expect(button.getAttribute("title")).toBe("New terminal");
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
  });

  it("renders without a leader key outside the provider", () => {
    // Isolated component tests should not require this context.
    render(<Probe />);

    const button = screen.getByRole("button", { name: "new" });
    expect(button.hasAttribute("aria-keyshortcuts")).toBe(false);
    expect(button.getAttribute("title")).toBe("New terminal");
  });

  it("does not render an empty aria-keyshortcuts attribute", () => {
    // An empty `aria-keyshortcuts` value is invalid even when the type allows it.
    mount(DEFAULT_LEADER);

    for (const node of document.querySelectorAll("[aria-keyshortcuts]")) {
      expect(node.getAttribute("aria-keyshortcuts")?.trim()).toBeTruthy();
    }
  });
});
