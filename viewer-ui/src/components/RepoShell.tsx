import { Suspense, lazy, useEffect } from "react";
import type { CSSProperties } from "react";
import { MAX_SIDEBAR_VIEWPORT_FRACTION } from "../hooks/ui/sidebar";
import { Sidebar } from "./Sidebar";
import type { SidebarProps } from "./Sidebar";
import { FilePane } from "./FilePane";
import type { FilePaneProps } from "./FilePane";
import type { Repo, Status } from "../api";
import type { ShellLayout } from "../hooks/useShellLayout";
import type { Maximized, MobileView } from "../types";
import { RepoMobileNav } from "./RepoMobileNav";
import { ErrorBoundary } from "./feedback/ErrorBoundary";

// Keep xterm out of the initial login and git-viewer bundle.
const TerminalPanel = lazy(() =>
  import("./terminal/Terminal").then((m) => ({ default: m.TerminalPanel })),
);

export interface RepoShellProps {
  repository: {
    id: string;
    current: Repo | undefined;
    status: Status | null;
  };
  sidebar: Omit<
    SidebarProps,
    | "sidebarRef"
    | "draggingSidebar"
    | "onSidebarDragStart"
    | "onSidebarDragMove"
    | "onSidebarDragEnd"
    | "onSidebarDragCancel"
    | "filesMax"
    | "mobileView"
    | "repo"
    | "status"
  >;
  filePane: Pick<
    FilePaneProps,
    "pane" | "previewRendered" | "setPreviewRendered" | "showOtherFace" | "repo"
  >;
  layout: ShellLayout & {
    maximized: Maximized;
    setMaximized: (
      next: Maximized | ((previous: Maximized) => Maximized),
    ) => void;
    mobileView: MobileView;
    setMobileView: (view: MobileView) => void;
  };
}

export function RepoShell({
  repository: { id: repo, current, status },
  sidebar,
  filePane,
  layout: {
    sidebarWidth,
    sidebarRef,
    draggingSidebar,
    onSidebarDragStart,
    onSidebarDragMove,
    onSidebarDragEnd,
    onSidebarDragCancel,
    upperRef,
    lowerRef,
    draggingUpper,
    onUpperDragStart,
    onUpperDragMove,
    onUpperDragEnd,
    onUpperDragCancel,
    maximized,
    setMaximized,
    mobileView,
    setMobileView,
  },
}: RepoShellProps) {
  const filesMax = maximized === "files";
  // The drag separator lives inside the keyed `Sidebar`, and the project can
  // change without the user letting go — another device switches it. The
  // separator then unmounts mid-drag and its pointerup never arrives, leaving
  // the overlay below to swallow every click for good.
  useEffect(() => onSidebarDragCancel, [repo, onSidebarDragCancel]);

  // The panel-split divider needs the same cleanup for a narrower case, and on
  // a narrower trigger. It lives in the terminal panel, which is *not* keyed by
  // repository, so a project switch leaves it mounted and its pointerup still
  // arrives — cancelling on every switch would abort a drag the user is in the
  // middle of. But closing the last project unmounts this whole shell, and then
  // the release never comes: the guard that stops a poll from adopting a split
  // mid-drag would stay raised for the rest of the page's life, and this
  // preference would never sync again. Depending only on the (stable) callback
  // makes this cleanup run on unmount and nowhere else.
  useEffect(() => onUpperDragCancel, [onUpperDragCancel]);

  return (
    <>
      {/* Keep the resize cursor and prevent selection during dragging. */}
      {draggingSidebar && <div className="fixed inset-0 z-50 cursor-col-resize" />}
      {draggingUpper && <div className="fixed inset-0 z-50 cursor-row-resize" />}
      <main
        ref={upperRef}
        className={`grid min-h-0 grid-cols-1 md:grid-cols-[var(--nc-sidebar)_1fr] ${
          mobileView === "terminal" ? "hidden md:grid" : ""
        } ${draggingSidebar || draggingUpper ? "select-none" : ""}`}
        style={
          {
            "--nc-sidebar": filesMax
              ? "0px"
              : `min(${sidebarWidth}px, ${MAX_SIDEBAR_VIEWPORT_FRACTION * 100}vw)`,
          } as CSSProperties
        }
      >
        {/* Keyed by repository so the file tree it holds — listings and
            expanded directories, all keyed by repository-relative path — goes
            away with the project. Two projects that share a directory name
            share a key, and the tree does not refetch a path it already holds,
            so a cache that outlived the switch would show one project's files
            under the other until a reload. */}
        <Sidebar
          key={repo}
          {...sidebar}
          repo={repo}
          status={status}
          sidebarRef={sidebarRef}
          draggingSidebar={draggingSidebar}
          onSidebarDragStart={onSidebarDragStart}
          onSidebarDragMove={onSidebarDragMove}
          onSidebarDragEnd={onSidebarDragEnd}
          onSidebarDragCancel={onSidebarDragCancel}
          filesMax={filesMax}
          mobileView={mobileView}
        />
        <FilePane
          {...filePane}
          filesMax={filesMax}
          setMaximized={setMaximized}
          status={status}
          className={mobileView === "diff" ? "flex" : "hidden md:flex"}
        />
      </main>

      {/*
        The fallback stands in the panel's grid row, so it takes the panel's
        visibility with it: below `md` this row is only on screen when the
        terminal is the chosen view, and a failure must not be what puts it there.
      */}
      <ErrorBoundary
        region="terminal panel"
        className={mobileView === "terminal" ? "flex" : "hidden md:flex"}
      >
        <Suspense fallback={null}>
          <TerminalPanel
            repo={repo}
            maximized={maximized === "terminal"}
            onToggleMaximized={() =>
              setMaximized((m) => (m === "terminal" ? "none" : "terminal"))
            }
            className={mobileView === "terminal" ? "flex" : "hidden md:flex"}
            sectionRef={lowerRef}
            // Only when both panels are on screen at their stored ratio: a
            // maximized panel has literal grid tracks the percentage does not
            // feed, and below `md` one view fills the screen instead.
            showDivider={maximized === "none"}
            draggingUpper={draggingUpper}
            onUpperDragStart={onUpperDragStart}
            onUpperDragMove={onUpperDragMove}
            onUpperDragEnd={onUpperDragEnd}
            onUpperDragCancel={onUpperDragCancel}
          />
        </Suspense>
      </ErrorBoundary>

      <RepoMobileNav view={mobileView} onSelect={setMobileView} />

      <footer className="flex shrink-0 items-center gap-3 border-t border-ink-700 bg-ink-900 px-3 py-1 text-ink-400">
        <span className="truncate">{current?.display_path}</span>
        {status?.branch && <span className="text-accent">{status.branch}</span>}
        {status?.tracking && (
          <span>
            ↑{status.tracking.ahead} ↓{status.tracking.behind}
          </span>
        )}
        <span className="ml-auto">
          {status ? <span className="text-added">● live</span> : "connecting…"}
        </span>
      </footer>
    </>
  );
}
