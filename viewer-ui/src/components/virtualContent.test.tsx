// @vitest-environment happy-dom

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Diff, DiffLine, Span } from "../api";
import { DiffView } from "./DiffView";
import { VirtualDiffView } from "./VirtualDiffView";
import { VirtualFileLines } from "./VirtualFileLines";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const line = (index: number, kind = " "): DiffLine => ({
  kind,
  spans: [{ t: `line ${index}`, c: "#fff" }],
  old_lineno: index,
  new_lineno: index,
});

function diffWith(count: number): Diff {
  return {
    path: "large.ts",
    hunks: [{ header: "@@ -1 +1 @@", lines: Array.from({ length: count }, (_, i) => line(i + 1)) }],
    truncated: false,
  };
}

function wideMatchMedia() {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query) =>
      ({
        matches: true,
        media: query,
        onchange: null,
        addListener: vi.fn(),
        removeListener: vi.fn(),
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
        dispatchEvent: vi.fn(),
      }) as MediaQueryList,
  );
}

describe("viewport content", () => {
  it("20k_diff는_viewport와_overscan_행만_DOM에_둔다", () => {
    const diff = diffWith(20_000);
    const memory = (globalThis as typeof globalThis & {
      process?: { memoryUsage(): { heapUsed: number } };
    }).process;
    const heapBefore = memory?.memoryUsage().heapUsed ?? 0;
    const initialStarted = performance.now();
    const view = render(
      <VirtualDiffView
        diff={diff}
        split={false}
        viewport={{ scrollTop: 0, height: 400 }}
      />,
    );
    const initialMs = performance.now() - initialStarted;
    expect(view.container.querySelector("[data-virtual-count]")?.getAttribute("data-virtual-count")).toBe("20001");
    expect(view.container.querySelectorAll("[data-virtual-row]").length).toBeLessThan(40);

    const scrollStarted = performance.now();
    view.rerender(
      <VirtualDiffView
        diff={diff}
        split={false}
        viewport={{ scrollTop: 200_000, height: 400 }}
      />,
    );
    const scrollMs = performance.now() - scrollStarted;
    const indices = Array.from(
      view.container.querySelectorAll<HTMLElement>("[data-virtual-row]"),
      (row) => Number(row.dataset.virtualRow),
    );
    expect(Math.min(...indices)).toBeGreaterThan(9_900);
    expect(Math.max(...indices)).toBeLessThan(10_100);
    const splitStarted = performance.now();
    view.rerender(
      <VirtualDiffView
        diff={diff}
        split
        viewport={{ scrollTop: 200_000, height: 400 }}
      />,
    );
    const splitMs = performance.now() - splitStarted;
    const heapDelta = (memory?.memoryUsage().heapUsed ?? 0) - heapBefore;
    console.info("20k virtualization metrics", {
      initial_ms: Number(initialMs.toFixed(2)),
      scroll_ms: Number(scrollMs.toFixed(2)),
      split_ms: Number(splitMs.toFixed(2)),
      long_tasks_over_50_ms: [initialMs, scrollMs, splitMs].filter((ms) => ms > 50).length,
      heap_delta_bytes: heapDelta,
      rendered_rows: view.container.querySelectorAll("[data-virtual-row]").length,
    });
  });

  it("멀리_스크롤한_행도_정확한_hunk_anchor를_가진다", () => {
    const diff: Diff = {
      path: "large.ts",
      hunks: [
        { header: "@@ first", lines: Array.from({ length: 10_000 }, (_, i) => line(i + 1)) },
        { header: "@@ second", lines: Array.from({ length: 10_000 }, (_, i) => line(i + 10_001)) },
      ],
      truncated: false,
    };
    const view = render(
      <VirtualDiffView
        diff={diff}
        split={false}
        viewport={{ scrollTop: 201_000, height: 400 }}
      />,
    );
    const hunks = Array.from(
      view.container.querySelectorAll<HTMLElement>("[data-hunk]"),
      (row) => row.dataset.hunk,
    );
    expect(new Set(hunks)).toEqual(new Set(["1"]));
  });

  it("split은_한_virtual_row에_old와_new를_정렬한다", () => {
    wideMatchMedia();
    const removed = Array.from({ length: 10_000 }, (_, i) => line(i + 1, "-"));
    const added = Array.from({ length: 10_000 }, (_, i) => line(i + 1, "+"));
    const diff: Diff = {
      path: "large.ts",
      hunks: [{ header: "@@", lines: [...removed, ...added] }],
      truncated: false,
    };
    const view = render(
      <VirtualDiffView diff={diff} split viewport={{ scrollTop: 20, height: 200 }} />,
    );
    const pair = view.container.querySelector<HTMLElement>("[data-virtual-row='1']");
    expect(pair?.textContent).toContain("-");
    expect(pair?.textContent).toContain("+");
    expect(view.container.querySelectorAll("[data-virtual-row]").length).toBeLessThan(40);
  });

  it("20k_file도_행_번호_anchor와_DOM_상한을_유지한다", () => {
    const lines: Span[][] = Array.from({ length: 20_000 }, (_, i) => [
      { t: `line ${i + 1}`, c: "#fff" },
    ]);
    const view = render(
      <VirtualFileLines lines={lines} viewport={{ scrollTop: 200_000, height: 400 }} />,
    );
    expect(view.container.querySelectorAll("[data-line]").length).toBeLessThan(50);
    expect(view.container.querySelector("[data-line='10001']")).not.toBeNull();
  });

  it("작은_diff는_전체_DOM과_선택_가능한_기존_경로를_쓴다", () => {
    const view = render(<DiffView diff={diffWith(3)} split={false} />);
    expect(view.container.querySelector("[data-virtual-count]")).toBeNull();
    expect(view.getByText("line 1").closest(".whitespace-pre")).not.toBeNull();
    expect(view.container.querySelectorAll("[data-hunk]")).toHaveLength(1);
  });
});
