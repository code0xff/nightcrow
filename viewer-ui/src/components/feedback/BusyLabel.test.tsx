// @vitest-environment happy-dom

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { BusyLabel } from "./BusyLabel";

afterEach(cleanup);

describe("BusyLabel", () => {
  it("작업중에도_글자는_그대로이고_점만_보인다", () => {
    const { container, rerender } = render(<BusyLabel busy={false}>Clone</BusyLabel>);
    const dot = () => container.querySelector("[data-busy]") as HTMLElement;
    expect(container.textContent).toBe("Clone");
    expect(dot().className).toContain("invisible");

    rerender(<BusyLabel busy>Clone</BusyLabel>);
    expect(container.textContent).toBe("Clone");
    expect(dot().className).toContain("nc-busy-dot");
    expect(dot().className).not.toContain("invisible");
    expect(dot().getAttribute("aria-hidden")).toBe("true");
  });
});
