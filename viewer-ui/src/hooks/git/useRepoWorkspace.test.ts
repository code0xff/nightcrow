// @vitest-environment happy-dom
//
// One control for "show this list". The keyboard (`view.toggleLog`,
// `view.toggleTree`) and the sidebar's tab row are two ways to ask for the same
// thing, and the ask is more than setting the tab: it invalidates the pane
// request in flight, empties the content pane the previous list filled, and
// drops the log's snapshot on the way out — exactly what the TUI's
// `toggle_mode` does. A keyboard wired to a weaker setter looks identical until
// somebody switches lists with a diff open.

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRepoWorkspace } from "./useRepoWorkspace";
import type { ShellLayout } from "../ui/useShellLayout";
import { api, type Diff, type RepoView } from "../../api";
import type { Maximized, Pane } from "../../types";
import { stubLocalStorage } from "../../lib/shared/fakeStorage";

vi.mock("../../api", () => ({
  api: {
    log: vi.fn(() => Promise.resolve({ commits: [], truncated: false })),
    setRepoView: vi.fn(() => Promise.resolve({})),
    diff: vi.fn(() => new Promise(() => {})),
    file: vi.fn(() => new Promise(() => {})),
  },
  subscribeStatus: vi.fn(() => () => {}),
  isUnauthorized: () => false,
}));

beforeEach(stubLocalStorage);

afterEach(() => {
  cleanup();
  localStorage.clear();
});

/** A content pane with something in it, so emptying it is observable. */
const OPEN_DIFF: Pane = {
  kind: "diff",
  value: { path: "a.ts", hunks: [], binary: false } as unknown as Diff,
};

function mount(remember = vi.fn(), remembered?: RepoView) {
  let maximized: Maximized = "none";
  return renderHook(() =>
    useRepoWorkspace({
      project: {
        repo: "r1",
        repos: [{ id: "r1", name: "r1", display_path: "~/r1" }],
        // False on purpose: every fetch in the subtree is gated on it, so the
        // seam under test runs with no network at all.
        authed: false,
        hot: null,
        clockSkewMs: null,
        resumeTick: 0,
        handle: vi.fn(),
      },
      view: {
        known: true,
        remembered,
        latest: () => undefined,
        remember,
      },
      layout: {
        shell: {} as ShellLayout,
        maximizedPanelOf: () => maximized,
        setMaximizedFor: (_repo, next) => {
          maximized = typeof next === "function" ? next(maximized) : next;
        },
      },
    }),
  );
}

describe("useRepoWorkspace list selection", () => {
  it("keyboard and tab rows use the same control", () => {
    // Identity, not equivalence: two functions that merely look alike are two
    // places for the sequence to drift.
    const { result } = mount();

    expect(result.current.repoShell).not.toBeNull();
    expect(result.current.chooseTab).toBe(
      result.current.repoShell!.sidebar.chooseTab,
    );
  });

  it("changing the list clears the open pane", () => {
    const remember = vi.fn();
    const { result } = mount(remember);
    act(() => result.current.setPane(OPEN_DIFF));
    expect(result.current.repoShell!.filePane.pane.kind).toBe("diff");
    remember.mockClear();

    act(() => result.current.chooseTab("tree"));

    expect(result.current.repoShell!.sidebar.tab).toBe("tree");
    expect(result.current.repoShell!.filePane.pane.kind).toBe("empty");
    // Both halves of the choice are recorded: the list and the emptied pane.
    expect(remember.mock.calls.map(([, view]) => view)).toEqual([
      expect.objectContaining({ tab: "tree" }),
      expect.objectContaining({ file: null }),
    ]);
  });

  it("selecting the same list again leaves the pane untouched", () => {
    const { result } = mount();
    act(() => result.current.setPane(OPEN_DIFF));

    act(() => result.current.chooseTab("status"));

    expect(result.current.repoShell!.filePane.pane.kind).toBe("diff");
  });

  it.each(["openDiff", "openFile"] as const)(
    "opening content with %s selects the mobile content view",
    (action) => {
      const { result } = mount();

      act(() => result.current.repoShell!.sidebar[action]("src/main.ts"));

      expect(result.current.repoShell!.layout.mobileView).toBe("diff");
    },
  );

  it("the terminal view survives remounts and restoring a remembered file", async () => {
    const remembered: RepoView = {
      tab: "status",
      file: { path: "src/main.ts", commit: null, face: "source" },
      tree_expanded: [],
    };
    vi.mocked(api.file).mockResolvedValue({
      path: "src/main.ts",
      lines: [],
      truncated: false,
    });

    const first = mount(vi.fn(), remembered);
    await act(async () => {
      await Promise.resolve();
    });
    act(() => first.result.current.repoShell!.layout.setMobileView("terminal"));
    first.unmount();

    const refreshed = mount(vi.fn(), remembered);
    await act(async () => {
      await Promise.resolve();
    });

    expect(refreshed.result.current.repoShell!.filePane.pane.kind).toBe("file");
    expect(refreshed.result.current.repoShell!.layout.mobileView).toBe("terminal");
  });
});
