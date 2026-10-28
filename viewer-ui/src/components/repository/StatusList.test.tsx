// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { StatusList } from "./StatusList";

afterEach(cleanup);

const props = { files: [], now: 0, hotWindowMs: 0, openDiff: () => {} };

describe("StatusList", () => {
  it("shows a skeleton while waiting for status", () => {
    const { container } = render(<ul><StatusList {...props} status={null} /></ul>);
    expect(container.querySelector(".nc-skeleton")).not.toBeNull();
  });

  it("shows why a tree cannot be read instead of a skeleton", () => {
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
