// @vitest-environment happy-dom

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HoverTitle } from "./HoverTitle";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function widths(scroll: number, client: number) {
  vi.spyOn(HTMLElement.prototype, "scrollWidth", "get").mockReturnValue(scroll);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(client);
}

describe("HoverTitle", () => {
  it("stays still and keeps the full text when it does not overflow", () => {
    widths(80, 120);
    const { container } = render(<HoverTitle title="short" />);
    const title = container.querySelector(".nc-title") as HTMLElement;
    expect(title.dataset.overflow).toBe("false");
    expect(title.textContent).toBe("short");
  });

  it("when it overflows, includes scroll distance and duration and keeps the full text without an ellipsis", () => {
    widths(300, 120);
    const name = "a-very-long-project-name-that-overflows";
    const { container } = render(<HoverTitle title={name} />);
    const title = container.querySelector(".nc-title") as HTMLElement;
    expect(title.dataset.overflow).toBe("true");
    expect(title.style.getPropertyValue("--nc-title-travel")).toBe("-180px");
    expect(title.style.getPropertyValue("--nc-title-duration")).toBe("6s");
    expect(title.textContent).toBe(name);
  });
});
