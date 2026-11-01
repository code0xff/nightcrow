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

const props = {
  files: [],
  clockSkewMs: 0,
  hotWindowMs: 0,
  viewport: { scrollTop: 0, height: 0 },
  openDiff: () => {},
};

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
            viewport={{ scrollTop: 0, height: 0 }}
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

describe("StatusList virtualization", () => {
  function files(count: number) {
    return Array.from({ length: count }, (_, i) => ({
      path: `f/${i}.ts`,
      index: "M",
      worktree: " ",
    }));
  }

  it("a large status renders only the viewport and its overscan", () => {
    const many = files(2_000);
    const view = render(
      <ul>
        <StatusList
          status={{ files: many, truncated: false }}
          files={many}
          hotWindowMs={0}
          clockSkewMs={0}
          viewport={{ scrollTop: 20_000, height: 400 }}
          openDiff={() => {}}
        />
      </ul>,
    );
    expect(
      view.container.querySelector("[data-virtual-count]")?.getAttribute("data-virtual-count"),
    ).toBe("2000");
    expect(view.container.querySelectorAll("[data-virtual-row]").length).toBeLessThan(50);
    expect(view.container.querySelector("[data-virtual-row='1000']")).not.toBeNull();
    expect(view.container.querySelector("[data-virtual-row='0']")).toBeNull();
  });

  it("keeps the row under the viewport clickable", () => {
    const many = files(2_000);
    const opened: string[] = [];
    const view = render(
      <ul>
        <StatusList
          status={{ files: many, truncated: false }}
          files={many}
          hotWindowMs={0}
          clockSkewMs={0}
          viewport={{ scrollTop: 20_000, height: 400 }}
          openDiff={(path) => opened.push(path)}
        />
      </ul>,
    );
    const row = view.container.querySelector<HTMLElement>("[data-virtual-row='1000']");
    row?.querySelector("button")?.click();
    expect(opened).toEqual(["f/1000.ts"]);
  });

  it("a small status keeps every row in the DOM without spacers", () => {
    const few = files(3);
    const view = render(
      <ul>
        <StatusList
          status={{ files: few, truncated: false }}
          files={few}
          hotWindowMs={0}
          clockSkewMs={0}
          viewport={{ scrollTop: 0, height: 400 }}
          openDiff={() => {}}
        />
      </ul>,
    );
    expect(view.container.querySelector("[data-virtual-count]")).toBeNull();
    expect(view.container.querySelectorAll("li")).toHaveLength(3);
  });
});
