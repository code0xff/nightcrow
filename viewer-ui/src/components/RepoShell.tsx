import { Suspense, lazy, useEffect } from "react";
import type { CSSProperties } from "react";
import { MAX_SIDEBAR_VIEWPORT_FRACTION } from "../hooks/ui/sidebar";
import type { PointerEvent as ReactPointerEvent } from "react";
import { Sidebar } from "./Sidebar";
import { FilePane } from "./FilePane";
import type { ChangedFile, Commit, Repo, Status } from "../api";
import type { CommitDrillDown } from "../hooks/useLog";
import type { Maximized, Pane, Tab } from "../types";
import type { MobileView } from "../types";
import { FileTextIcon, ListIcon, TerminalIcon } from "./icons";

// Keep xterm out of the initial login and git-viewer bundle.
const TerminalPanel = lazy(() =>
  import("./terminal/Terminal").then((m) => ({ default: m.TerminalPanel })),
);

export interface RepoShellProps {
  repo: string;
  repos: Repo[];
  current: Repo | undefined;
  status: Status | null;
  files: ChangedFile[];
  now: number;
  hotWindowMs: number;
  pane: Pane;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  tab: Tab;
  setTab: (t: Tab) => void;
  filter: string;
  setFilter: (v: string) => void;
  filterOpen: boolean;
  setFilterOpen: React.Dispatch<React.SetStateAction<boolean>>;
  openDiff: (path: string) => void;
  openFile: (path: string) => void;
  openCommit: (oid: string) => void;
  openCommitFileDiff: (oid: string, path: string) => void;
  openCommitFiles: (commit: Commit) => void;
  authed: boolean | null;
  handle: (err: unknown) => void;
  sidebarWidth: number;
  sidebarRef: React.RefObject<HTMLElement | null>;
  draggingSidebar: boolean;
  onSidebarDragStart: (e: ReactPointerEvent) => void;
  onSidebarDragMove: (e: ReactPointerEvent) => void;
  onSidebarDragEnd: () => void;
  onSidebarDragCancel: () => void;
  /** The diff panel and the terminal panel — the two edges of the vertical
   *  split the divider between them measures against. */
  upperRef: React.RefObject<HTMLElement | null>;
  lowerRef: React.RefObject<HTMLElement | null>;
  draggingUpper: boolean;
  onUpperDragStart: (e: ReactPointerEvent) => void;
  onUpperDragMove: (e: ReactPointerEvent) => void;
  onUpperDragEnd: () => void;
  onUpperDragCancel: () => void;
  filesMax: boolean;
  bumpPaneRequest: () => void;
  commits: Commit[];
  logDone: boolean;
  logStalled: boolean;
  setLogStalled: (v: boolean | ((prev: boolean) => boolean)) => void;
  commitDrillDown: CommitDrillDown | null;
  setCommitDrillDown: (v: CommitDrillDown | null) => void;
  resetLog: () => void;
  logSentinelRef: React.RefObject<HTMLLIElement | null>;
  visibleCommits: Commit[];
  logPagingPaused: boolean;
  aheadOids: Set<string>;
  visibleCommitFiles: CommitDrillDown["files"];
  previewRendered: boolean;
  setPreviewRendered: React.Dispatch<React.SetStateAction<boolean>>;
  maximized: Maximized;
  setMaximized: (next: Maximized | ((prev: Maximized) => Maximized)) => void;
  mobileView: MobileView;
  setMobileView: (view: MobileView) => void;
}

export function RepoShell(props: RepoShellProps) {
  const {
    repo,
    current,
    status,
    files,
    now,
    hotWindowMs,
    pane,
    setPane,
    tab,
    setTab,
    filter,
    setFilter,
    filterOpen,
    setFilterOpen,
    openDiff,
    openFile,
    openCommit,
    openCommitFileDiff,
    openCommitFiles,
    authed,
    handle,
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
    filesMax,
    bumpPaneRequest,
    commits,
    logDone,
    logStalled,
    setLogStalled,
    commitDrillDown,
    setCommitDrillDown,
    resetLog,
    logSentinelRef,
    visibleCommits,
    logPagingPaused,
    aheadOids,
    visibleCommitFiles,
    previewRendered,
    setPreviewRendered,
    maximized,
    setMaximized,
    mobileView,
    setMobileView,
  } = props;

  // The drag separator lives inside the keyed `Sidebar`, and the project can
  // change without the user letting go — another device switches it. The
  // separator then unmounts mid-drag and its pointerup never arrives, leaving
  // the overlay below to swallow every click for good.
  useEffect(() => onSidebarDragCancel, [repo, onSidebarDragCancel]);

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
          tab={tab}
          setTab={setTab}
          filter={filter}
          setFilter={setFilter}
          filterOpen={filterOpen}
          setFilterOpen={setFilterOpen}
          status={status}
          files={files}
          now={now}
          hotWindowMs={hotWindowMs}
          setPane={setPane}
          openDiff={openDiff}
          openFile={openFile}
          openCommit={openCommit}
          openCommitFileDiff={openCommitFileDiff}
          openCommitFiles={openCommitFiles}
          repo={repo}
          authed={authed}
          handle={handle}
          sidebarRef={sidebarRef}
          draggingSidebar={draggingSidebar}
          onSidebarDragStart={onSidebarDragStart}
          onSidebarDragMove={onSidebarDragMove}
          onSidebarDragEnd={onSidebarDragEnd}
          onSidebarDragCancel={onSidebarDragCancel}
          filesMax={filesMax}
          bumpPaneRequest={bumpPaneRequest}
          commits={commits}
          logDone={logDone}
          logStalled={logStalled}
          setLogStalled={setLogStalled}
          commitDrillDown={commitDrillDown}
          setCommitDrillDown={setCommitDrillDown}
          resetLog={resetLog}
          logSentinelRef={logSentinelRef}
          visibleCommits={visibleCommits}
          logPagingPaused={logPagingPaused}
          aheadOids={aheadOids}
          visibleCommitFiles={visibleCommitFiles}
          mobileView={mobileView}
        />
        <FilePane
          pane={pane}
          previewRendered={previewRendered}
          setPreviewRendered={setPreviewRendered}
          filesMax={filesMax}
          setMaximized={setMaximized}
          status={status}
          className={mobileView === "diff" ? "flex" : "hidden md:flex"}
        />
      </main>

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

      <nav
        aria-label="Switch view"
        className="flex shrink-0 items-stretch border-t border-ink-700 bg-ink-900 md:hidden"
      >
        {(
          [
            ["files", "Files", ListIcon],
            ["diff", "Diff", FileTextIcon],
            ["terminal", "Terminal", TerminalIcon],
          ] as [MobileView, string, typeof ListIcon][]
        ).map(([key, label, Icon]) => (
          <button
            key={key}
            onClick={() => setMobileView(key)}
            aria-current={mobileView === key ? "page" : undefined}
            className={`flex min-h-11 flex-1 flex-col items-center justify-center gap-0.5 py-1 text-[11px] ${
              mobileView === key
                ? "text-accent shadow-[inset_0_2px_0_0_var(--color-accent)]"
                : "text-ink-400"
            }`}
          >
            <Icon className="h-5 w-5" />
            {label}
          </button>
        ))}
      </nav>

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
