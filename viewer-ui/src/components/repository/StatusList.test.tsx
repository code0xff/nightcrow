// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StatusList } from "./StatusList";

afterEach(cleanup);

const props = { files: [], now: 0, hotWindowMs: 0, openDiff: () => {} };

describe("StatusList", () => {
  it("status를_기다리는_동안은_스켈레톤을_보인다", () => {
    const { container } = render(<ul><StatusList {...props} status={null} /></ul>);
    expect(container.querySelector(".nc-skeleton")).not.toBeNull();
  });

  it("읽을_수_없는_트리는_스켈레톤_대신_이유를_보인다", () => {
    const { container } = render(
      <ul>
        <StatusList
          {...props}
          status={{ files: [], truncated: false, unavailable: "not a git repository: x" }}
        />
      </ul>,
    );
    expect(container.querySelector(".nc-skeleton")).toBeNull();
    expect(screen.getByText(/Not a git repository/)).toBeTruthy();
  });
});
