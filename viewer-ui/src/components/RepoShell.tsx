import { Suspense, lazy } from "react";
import type { CSSProperties } from "react";
import { MAX_SIDEBAR_VIEWPORT_FRACTION } from "../sidebar";
import type { PointerEvent as ReactPointerEvent } from "react";
import { Sidebar } from "./Sidebar";
import { FilePane } from "./FilePane";
import type { ChangedFile, Commit, Repo, Status } from "../api";
import type { CommitDrillDown } from "../hooks/useLog";
import type { Maximized, Pane, Tab } from "../types";

// Keep xterm out of the initial login and git-viewer bundle.
const TerminalPanel = lazy(() =>
  import("../Terminal").then((m) => ({ default: m.TerminalPanel })),
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
  mdRendered: boolean;
  setMdRendered: React.Dispatch<React.SetStateAction<boolean>>;
  maximized: Maximized;
  setMaximized: (next: Maximized | ((prev: Maximized) => Maximized)) => void;
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
    mdRendered,
    setMdRendered,
    maximized,
    setMaximized,
  } = props;

  return (
    <>
      {/* Keep the resize cursor and prevent selection during dragging. */}
      {draggingSidebar && <div className="fixed inset-0 z-50 cursor-col-resize" />}
      <main
        className={`grid min-h-0 grid-cols-1 md:grid-cols-[var(--nc-sidebar)_1fr] ${
          draggingSidebar ? "select-none" : ""
        }`}
        style={
          {
            "--nc-sidebar": filesMax
              ? "0px"
              : `min(${sidebarWidth}px, ${MAX_SIDEBAR_VIEWPORT_FRACTION * 100}vw)`,
          } as CSSProperties
        }
      >
        <Sidebar
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
        />
        <FilePane
          pane={pane}
          mdRendered={mdRendered}
          setMdRendered={setMdRendered}
          filesMax={filesMax}
          setMaximized={setMaximized}
          status={status}
        />
      </main>

      <Suspense fallback={null}>
        <TerminalPanel
          repo={repo}
          maximized={maximized === "terminal"}
          onToggleMaximized={() =>
            setMaximized((m) => (m === "terminal" ? "none" : "terminal"))
          }
        />
      </Suspense>

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
