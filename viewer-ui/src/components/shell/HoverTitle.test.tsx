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
  it("넘치지_않으면_정지해_있고_전체_글자를_그대로_둔다", () => {
    widths(80, 120);
    const { container } = render(<HoverTitle title="short" />);
    const title = container.querySelector(".nc-title") as HTMLElement;
    expect(title.dataset.overflow).toBe("false");
    expect(title.textContent).toBe("short");
  });

  it("넘치면_넘친_만큼_흐를_거리와_시간을_싣고_말줄임표_없이_전체를_둔다", () => {
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
