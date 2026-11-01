// @vitest-environment happy-dom

import { act, cleanup, render, screen } from "@testing-library/react";
import { memo } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StatusList } from "./StatusList";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const props = { files: [], clockSkewMs: 0, hotWindowMs: 0, openDiff: () => {} };

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

describe("StatusList hot clock", () => {
  it("fades a row on its own tick without re-rendering the page beside it", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
    const files = [{ path: "a.ts", index: "M", worktree: " ", mtime: 1_000_000 }];
    const siblingRenders = vi.fn();
    const Sibling = memo(function Sibling() {
      siblingRenders();
      return <li>terminal</li>;
    });
    let pageRenders = 0;
    function Page() {
      pageRenders += 1;
      return (
        <ul>
          <StatusList
            status={{ files, truncated: false }}
            files={files}
            hotWindowMs={60_000}
            clockSkewMs={0}
            openDiff={() => {}}
          />
          <Sibling />
        </ul>
      );
    }
    const view = render(<Page />);
    const label = () => view.container.querySelector('span[title="a.ts"]');
    expect(label()?.className).toContain("font-bold");

    act(() => vi.advanceTimersByTime(6_000));
    expect(label()?.className).toContain("text-accent");
    expect(label()?.className).not.toContain("font-bold");

    act(() => vi.advanceTimersByTime(60_000));
    expect(label()?.className).not.toContain("text-accent");

    expect(pageRenders).toBe(1);
    expect(siblingRenders).toHaveBeenCalledTimes(1);
  });
});
