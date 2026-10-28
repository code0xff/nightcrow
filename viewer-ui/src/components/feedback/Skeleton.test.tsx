// @vitest-environment happy-dom

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { SkeletonRows } from "./Skeleton";

afterEach(cleanup);

describe("SkeletonRows", () => {
  it("renders the requested number of rows hidden from assistive technology", () => {
    const { container } = render(
      <ul>
        <SkeletonRows as="li" rows={4} />
      </ul>,
    );
    const rows = container.querySelectorAll("li");
    expect(rows).toHaveLength(4);
    rows.forEach((row) => expect(row.getAttribute("aria-hidden")).toBe("true"));
    expect(container.querySelectorAll(".nc-skeleton")).toHaveLength(8);
  });
});
