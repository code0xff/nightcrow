// @vitest-environment happy-dom

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { RepoStatus } from "../../hooks/git/useStatus";
import { RepoShell, type RepoShellProps } from "./RepoShell";

// The panel itself is xterm and the terminal protocol; the question here is
// only whether a parent render reaches it, so the lazy module is a counter.
const terminal = vi.hoisted(() => ({ renders: 0 }));

vi.mock("../terminal/Terminal", async () => {
  const { memo: memoize } = await import("react");
  return {
    TerminalPanel: memoize(function TerminalPanelStub() {
      terminal.renders += 1;
      return <section data-testid="terminal-stub" />;
    }),
  };
});

afterEach(() => {
  cleanup();
  terminal.renders = 0;
});

const noop = () => {};
// Stable like the ones the page holds: the terminal panel's memo compares
// them, and a fresh ref per render would hide what this harness measures.
const sidebarRef = { current: null };
const upperRef = { current: null };
const lowerRef = { current: null };
const status: RepoStatus = { files: [], truncated: false, branch: "dev" };
const decorations = {
  refsOf: () => undefined,
  divergenceOf: () => undefined,
  truncated: false,
};

function props(overrides: Partial<RepoShellProps> = {}): RepoShellProps {
  return {
    repository: {
      id: "repo-1",
      current: { id: "repo-1", name: "nightcrow", display_path: "~/nightcrow" },
      status,
    },
    sidebar: {
      tab: "status",
      chooseTab: noop,
      filter: "",
      setFilter: noop,
      filterOpen: false,
      setFilterOpen: noop,
      files: [],
      hotWindowMs: 0,
      clockSkewMs: 0,
      openDiff: noop,
      openFile: noop,
      openCommit: noop,
      openCommitFileDiff: noop,
      openCommitFiles: noop,
      authed: true,
      handle: noop,
      bumpPaneRequest: noop,
      commits: [],
      logDone: true,
      logStalled: false,
      setLogStalled: noop,
      commitDrillDown: null,
      setCommitDrillDown: noop,
      logSentinelRef: { current: null },
      visibleCommits: [],
      decorations,
      logPagingPaused: false,
      visibleCommitFiles: [],
      restoreTree: [],
      restoreKnown: true,
      onTreeExpanded: noop,
      clearPane: noop,
      touched: false,
    },
    filePane: {
      repo: "repo-1",
      pane: { kind: "empty" },
      loading: false,
      previewRendered: true,
      setPreviewRendered: noop,
      showOtherFace: noop,
    },
    layout: {
      sidebarWidth: 240,
      sidebarRef,
      draggingSidebar: false,
      onSidebarDragStart: noop,
      onSidebarDragMove: noop,
      onSidebarDragEnd: noop,
      onSidebarDragCancel: noop,
      upperRef,
      lowerRef,
      draggingUpper: false,
      onUpperDragStart: noop,
      onUpperDragMove: noop,
      onUpperDragEnd: noop,
      onUpperDragCancel: noop,
      maximized: "none",
      setMaximized: noop,
      mobileView: "files",
      setMobileView: noop,
    },
    ...overrides,
  };
}

describe("RepoShell terminal panel", () => {
  it("a status event does not re-render the terminal panel", async () => {
    const view = render(<RepoShell {...props()} />);
    // The panel is a lazy chunk; its first load can take a moment here.
    await screen.findByTestId("terminal-stub", {}, { timeout: 5_000 });
    expect(terminal.renders).toBe(1);

    // The SSE status stream replaces the whole object, not one field of it.
    const next: RepoStatus = { files: [], truncated: false, branch: "main" };
    view.rerender(
      <RepoShell
        {...props({ repository: { id: "repo-1", current: undefined, status: next } })}
      />,
    );
    expect(terminal.renders).toBe(1);
  });

  it("maximizing still reaches the panel through the stable callback", async () => {
    const view = render(<RepoShell {...props()} />);
    await screen.findByTestId("terminal-stub", {}, { timeout: 5_000 });
    const layout = props().layout;
    view.rerender(
      <RepoShell
        {...props({
          layout: { ...layout, maximized: "terminal" },
        })}
      />,
    );
    expect(terminal.renders).toBe(2);
  });
});
