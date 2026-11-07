// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ModifierLatch } from "../../hooks/terminal/useModifierLatch";
import { TermKeyBar } from "./TermKeyBar";

afterEach(cleanup);

function latch(): ModifierLatch {
  return {
    armed: false,
    toggle: () => false,
    clear: vi.fn(),
    consume: (typed) => typed,
  };
}

describe("TermKeyBar", () => {
  it("sends Shift+Left to the active pane and clears modifier latches", () => {
    const onKey = vi.fn();
    const ctrl = latch();
    const alt = latch();
    render(<TermKeyBar onKey={onKey} ctrl={ctrl} alt={alt} onArm={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "Shift Left arrow" }));

    expect(onKey).toHaveBeenCalledTimes(1);
    expect(onKey).toHaveBeenCalledWith("shift-left");
    expect(ctrl.clear).toHaveBeenCalledTimes(1);
    expect(alt.clear).toHaveBeenCalledTimes(1);
  });
});
