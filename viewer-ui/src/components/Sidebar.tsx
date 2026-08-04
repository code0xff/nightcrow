import type { PointerEvent as ReactPointerEvent } from "react";
import { SearchIcon } from "./icons/actions";
import { useTree } from "../hooks/useTree";
import { buildTreeRows } from "../lib/tree";
import { StatusList } from "./StatusList";
import { LogList } from "./LogList";
import { TreeList } from "./TreeList";
import type { ChangedFile, Commit, Status } from "../api";
import type { CommitDrillDown } from "../hooks/useLog";
import type { Pane, Tab } from "../types";
import type { MobileView } from "../types";

export interface SidebarProps {
  tab: Tab;
  setTab: (t: Tab) => void;
  filter: string;
  setFilter: (v: string) => void;
  filterOpen: boolean;
  setFilterOpen: React.Dispatch<React.SetStateAction<boolean>>;
  status: Status | null;
  files: ChangedFile[];
  now: number;
  hotWindowMs: number;
  setPane: React.Dispatch<React.SetStateAction<Pane>>;
  openDiff: (path: string) => void;
  openFile: (path: string) => void;
  openCommit: (oid: string) => void;
  openCommitFileDiff: (oid: string, path: string) => void;
  openCommitFiles: (commit: Commit) => void;
  repo: string | null;
  authed: boolean | null;
  handle: (err: unknown) => void;
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
  mobileView: MobileView;
}

export function Sidebar(props: SidebarProps) {
  const {
    tab,
    setTab,
    filter,
    setFilter,
    filterOpen,
    setFilterOpen,
    status,
    files,
    now,
    hotWindowMs,
    setPane,
    openDiff,
    openFile,
    openCommit,
    openCommitFileDiff,
    openCommitFiles,
    repo,
    authed,
    handle,
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
    mobileView,
  } = props;

  const tree = useTree({ repo, authed, tab, filter, filterOpen, handle });
  const treeSearching = tab === "tree" && filterOpen && filter !== "";
  const treeRows = buildTreeRows(tree.treeChildren, tree.treeExpanded);

  return (
    <section
      ref={sidebarRef}
      className={`relative min-h-0 flex-col overflow-hidden ${
        mobileView === "files" ? "flex" : "hidden md:flex"
      } ${filesMax ? "md:flex" : "border-ink-700 md:border-r"}`}
    >
      {!filesMax && (
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize the file sidebar (double-click to reset)"
          title="Drag to resize · double-click to reset"
          onPointerDown={onSidebarDragStart}
          onPointerMove={onSidebarDragMove}
          onPointerUp={onSidebarDragEnd}
          onPointerCancel={onSidebarDragCancel}
          onLostPointerCapture={onSidebarDragEnd}
          // A tier above the list's own sticky chrome, not level with it. The
          // commit drill-down's header is `sticky z-10` with a background, and at
          // equal z-index the later element in the DOM wins — so it painted over
          // the divider, and the highlight that says "you are dragging this" broke
          // for exactly the height of that header.
          className={`absolute -right-px top-0 z-20 hidden h-full w-1.5 cursor-col-resize touch-none md:block ${
            draggingSidebar ? "bg-accent" : "hover:bg-accent"
          }`}
        />
      )}
      <div className="flex shrink-0 items-stretch border-b border-ink-700 px-2">
        {(["status", "log", "tree"] as Tab[]).map((t) => (
          <button
            key={t}
            onClick={() => {
              if (t === tab) return;
              bumpPaneRequest();
              if (tab === "log") {
                setCommitDrillDown(null);
                // Re-entering the log must use a fresh history snapshot.
                resetLog();
              }
              setTab(t);
              setPane({ kind: "empty" });
            }}
            aria-current={t === tab ? "page" : undefined}
            className={`-mb-px border-b-2 px-2 py-1 ${
              t === tab
                ? "border-accent text-ink-50"
                : "border-transparent text-ink-400 hover:text-ink-200"
            }`}
          >
            {t}
          </button>
        ))}
        <button
          onClick={() => {
            if (filterOpen) setFilter("");
            setFilterOpen((open) => !open);
          }}
          aria-pressed={filterOpen}
          title={filterOpen ? "Hide the filter" : "Filter the list"}
          aria-label={filterOpen ? "Hide the filter" : "Filter the list"}
          className={`my-1 ml-auto flex shrink-0 items-center rounded-sm px-1.5 hover:text-accent ${
            filterOpen ? "text-ink-50" : "text-ink-400"
          }`}
        >
          <SearchIcon />
        </button>
      </div>
      {filterOpen && (
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder="filter…"
          autoFocus
          className="mx-2 mb-1 shrink-0 rounded-sm bg-ink-850 px-2 py-1 outline-none placeholder:text-ink-400 focus:ring-1 focus:ring-accent"
        />
      )}
      <ul className="min-h-0 flex-1 overflow-auto">
        {tab === "status" && (
          <StatusList
            status={status}
            files={files}
            now={now}
            hotWindowMs={hotWindowMs}
            openDiff={openDiff}
          />
        )}
        {tab === "log" && (
          <LogList
            visibleCommits={visibleCommits}
            commits={commits}
            aheadOids={aheadOids}
            commitDrillDown={commitDrillDown}
            visibleCommitFiles={visibleCommitFiles}
            logDone={logDone}
            logStalled={logStalled}
            logPagingPaused={logPagingPaused}
            setLogStalled={setLogStalled}
            logSentinelRef={logSentinelRef}
            openCommitFiles={openCommitFiles}
            openCommit={openCommit}
            openCommitFileDiff={openCommitFileDiff}
            setCommitDrillDown={setCommitDrillDown}
            setPaneEmpty={() => setPane({ kind: "empty" })}
            bumpPaneRequest={bumpPaneRequest}
          />
        )}
        {tab === "tree" && (
          <TreeList
            treeSearching={treeSearching}
            treeMatches={tree.treeMatches}
            treeTruncated={tree.treeTruncated}
            treeSearchLoading={tree.treeSearchLoading}
            treeRows={treeRows}
            treeExpanded={tree.treeExpanded}
            openFile={openFile}
            revealTreeDir={tree.revealTreeDir}
            toggleTreeDir={tree.toggleTreeDir}
          />
        )}
      </ul>
    </section>
  );
}
