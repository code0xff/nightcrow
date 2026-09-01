import { useEffect, useRef } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { focusRegionAttrs } from "../lib/shortcutDom";
import { useTree } from "../hooks/useTree";
import { buildTreeRows } from "../lib/tree";
import { ancestorDirs, toggled } from "../lib/treeCache";
import { SidebarTabs } from "./SidebarTabs";
import { StatusList } from "./StatusList";
import { LogList } from "./LogList";
import { TreeList } from "./TreeList";
import type { ChangedFile, Commit, Status } from "../api";
import type { CommitDrillDown } from "../hooks/useLog";
import type { Tab } from "../types";
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
  /** Directories the tree had open when this project was last looked at.
   *  Restored here rather than by the hook that restores the tab and the file,
   *  because the tree cache lives in this component — it is keyed by repository
   *  so the listings go with the project (`useTree`). */
  restoreTree: string[];
  /** Whether the server has answered about this project yet. Until it has, an
   *  empty `restoreTree` means "not asked", not "nothing was open". */
  restoreKnown: boolean;
  /** Report the shape a tap on a directory puts the tree in, so it is
   *  remembered with the rest of the view. Reported from the tap rather than
   *  from watching the cache: the cache is also what a restore plants, and a
   *  plant is not a choice. */
  onTreeExpanded: (dirs: string[]) => void;
  /** Leave the content pane showing nothing — coming back out of a commit's
   *  file list. A choice like any other, so it is recorded like one. */
  clearPane: () => void;
  /** Whether this project's screen already has a choice in it, in which case
   *  there is nothing to restore over. */
  touched: boolean;
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
    restoreTree,
    restoreKnown,
    onTreeExpanded,
    clearPane,
    touched,
  } = props;

  const tree = useTree({ repo, authed, tab, filter, filterOpen, handle });
  const treeSearching = tab === "tree" && filterOpen && filter !== "";
  const treeRows = buildTreeRows(tree.treeChildren, tree.treeExpanded);

  // Once per project. This component is keyed by repository, so a mount is a
  // project being opened — and reopening what someone has since collapsed would
  // be the tree undoing them.
  const { seedTreeExpanded } = tree;
  const restored = useRef(false);
  useEffect(() => {
    if (restored.current) return;
    // Not before the server has spoken for this project: an empty remembered
    // shape and one that has not arrived look the same from here. And not over
    // someone who has already been in this project — including in the window
    // before that response, which is exactly when this would otherwise fire.
    if (!restoreKnown || touched) return;
    // Only once the tree is what is being looked at. Planting it earlier would
    // fetch a listing per remembered directory for a screen showing the status
    // list, and the shape is worth nothing until it is on screen.
    if (tab !== "tree") return;
    // Once the server has spoken, an empty shape is a real answer — nothing was
    // open — and the restore is done. Marked done even then: a shape another
    // browser records later is news, not this visit's restore, and planting it
    // would reshape a tree someone here is already looking at.
    restored.current = true;
    if (restoreTree.length === 0) return;
    seedTreeExpanded(restoreTree);
  }, [tab, restoreKnown, touched, restoreTree, seedTreeExpanded]);

  // Every way the tree's shape changes by choice, each saying what it will be
  // rather than leaving it to be read off the cache afterwards — the cache is
  // also what a restore plants.
  const toggleAndNote = (path: string) => {
    onTreeExpanded([...toggled(tree.treeExpanded, path)]);
    tree.toggleTreeDir(path);
  };
  const revealAndNote = (path: string) => {
    const next = new Set(tree.treeExpanded);
    ancestorDirs(path).forEach((dir) => next.add(dir));
    onTreeExpanded([...next]);
    tree.revealTreeDir(path);
  };

  // Both halves of one choice: the list to show, and the pane it leaves empty
  // behind it. Named here rather than in the tab row because the log's reset is
  // this component's state.
  const chooseTab = (t: Tab) => {
    if (t === tab) return;
    bumpPaneRequest();
    if (tab === "log") {
      setCommitDrillDown(null);
      // Re-entering the log must use a fresh history snapshot.
      resetLog();
    }
    setTab(t);
    clearPane();
  };
  const toggleFilter = () => {
    if (filterOpen) setFilter("");
    setFilterOpen((open) => !open);
  };

  return (
    <section
      ref={sidebarRef}
      // The region `focus.list` sends the keyboard to. `tabIndex={-1}` because a
      // container is not focusable otherwise, and -1 keeps it out of the Tab
      // order: it is a shortcut target, not a stop.
      {...focusRegionAttrs("list")}
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
      <SidebarTabs
        tab={tab}
        onChoose={chooseTab}
        filterOpen={filterOpen}
        onToggleFilter={toggleFilter}
      />
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
            setPaneEmpty={clearPane}
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
            revealTreeDir={revealAndNote}
            toggleTreeDir={toggleAndNote}
          />
        )}
      </ul>
    </section>
  );
}
