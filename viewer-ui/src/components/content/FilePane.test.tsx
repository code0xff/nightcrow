// @vitest-environment happy-dom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FilePane } from "./FilePane";
import type { Diff } from "../../api";
import type { Pane } from "../../types";

vi.mock("./DiffView", () => ({ DiffView: () => null }));

afterEach(cleanup);

describe("FilePane scroll restoration", () => {
  it("restores a saved position only when returning to the same repository", () => {
    const pane: Pane = {
      kind: "diff",
      value: { path: "same.txt", hunks: [], truncated: false } satisfies Diff,
      source: { kind: "workdir", path: "same.txt" },
    };
    const props = {
      pane,
      previewRendered: true,
      setPreviewRendered: vi.fn(),
      filesMax: false,
      setMaximized: vi.fn(),
      showOtherFace: vi.fn(),
      status: null,
    };
    const { container, rerender } = render(<FilePane {...props} repo="repo-a" />);
    const scroller = container.querySelector(".nc-content") as HTMLDivElement;

    scroller.scrollTop = 180;
    fireEvent.click(screen.getByLabelText("Open the whole file at this change"));
    expect(scroller.scrollTop).toBe(180);

    scroller.scrollTop = 0;
    rerender(<FilePane {...props} repo="repo-b" />);
    expect(scroller.scrollTop).toBe(0);
    rerender(<FilePane {...props} repo="repo-a" />);

    expect(scroller.scrollTop).toBe(180);
  });
});
