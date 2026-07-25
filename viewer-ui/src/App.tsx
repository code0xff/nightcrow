import { useCallback, useEffect, useRef, useState } from "react";
import { isUnauthorized } from "./api";
import { toast } from "./toast";
import { useAccent } from "./theme";
import { useSidebarWidth } from "./sidebar";
import { useHotClock } from "./useHotClock";
import { useRepoPoll } from "./hooks/useRepoPoll";
import { useStatus } from "./hooks/useStatus";
import { useSidebarDrag } from "./hooks/useSidebarDrag";
import { usePaneOpeners } from "./hooks/usePaneOpeners";
import { useLog } from "./hooks/useLog";
import { useResumeTick } from "./hooks/useResumeTick";
import { useMaximized } from "./hooks/useMaximized";
import { useRepoActions } from "./hooks/useRepoActions";
import { Header } from "./components/Header";
import { RepoShell } from "./components/RepoShell";
import { FolderPicker } from "./components/FolderPicker";
import { LoadingSplash } from "./components/LoadingSplash";
import { Login } from "./components/Login";
import type { Pane, Tab } from "./types";

export function App() {
  const [authed, setAuthed] = useState<boolean | null>(null);
  const [tab, setTab] = useState<Tab>("status");
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [pane, setPane] = useState<Pane>({ kind: "empty" });
  // Prevent stale pane responses from overwriting the new context.
  const paneRequestRef = useRef(0);
  const bumpPaneRequest = useCallback(() => {
    paneRequestRef.current += 1;
  }, []);
  const [pickerOpen, setPickerOpen] = useState(false);
  const { accent, next, cycle: cycleAccent, adopt: adoptAccent } = useAccent();
  const accentWrites = useRef(0);
  const cycle = useCallback(() => {
    accentWrites.current += 1;
    cycleAccent();
  }, [cycleAccent]);
  const {
    width: sidebarWidth,
    resize: resizeSidebar,
    commit: commitSidebar,
    reset: resetSidebar,
    adopt: adoptSidebarWidth,
  } = useSidebarWidth();
  const sidebarWrites = useRef(0);
  const commitSidebarWidth = useCallback(
    (px: number) => {
      sidebarWrites.current += 1;
      commitSidebar(px);
    },
    [commitSidebar],
  );
  const resetSidebarWidth = useCallback(() => {
    sidebarWrites.current += 1;
    resetSidebar();
  }, [resetSidebar]);
  const bumpSidebarWrites = useCallback(() => {
    sidebarWrites.current += 1;
  }, []);
  const sidebarRef = useRef<HTMLElement>(null);
  const [mdRendered, setMdRendered] = useState(true);

  const handle = useCallback((err: unknown) => {
    if (isUnauthorized(err)) {
      setAuthed(false);
      return;
    }
    toast.error(err instanceof Error ? err.message : "request failed");
  }, []);

  const resumeTick = useResumeTick();

  const {
    draggingSidebar,
    onSidebarDragStart,
    onSidebarDragMove,
    onSidebarDragEnd,
    onSidebarDragCancel,
    draggingRef,
  } = useSidebarDrag({
    sidebarRef,
    sidebarWidth,
    resizeSidebar,
    commitSidebarWidth,
    resetSidebarWidth,
    bumpSidebarWrites,
  });

  const {
    repos,
    setRepos,
    repo,
    setRepo,
    hot,
    clockSkewMs,
    reposLoaded,
  } = useRepoPoll({
    authed,
    setAuthed,
    handle,
    adoptAccent,
    adoptSidebarWidth,
    draggingRef,
    accentWrites,
    sidebarWrites,
    resumeTick,
  });

  const hotWindowMs = hot?.enabled ? hot.window_secs * 1000 : 0;
  useHotClock(undefined, hotWindowMs, clockSkewMs ?? 0);

  const { status } = useStatus({
    repo,
    authed,
    resumeTick,
    tab,
    pane,
    setPane,
    handle,
    paneRequestRef,
  });
  const now = useHotClock(status?.files, hotWindowMs, clockSkewMs ?? 0);

  const { maximized, setMaximized, dropMaximized } = useMaximized(repo);

  const {
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
  } = useLog({ repo, authed, tab, filter, handle });

  const { openDiff, openFile, openCommit, openCommitFileDiff, openCommitFiles } =
    usePaneOpeners({
      repo,
      handle,
      setPane,
      paneRequestRef,
      setCommitDrillDown,
    });

  const { selectOpenedRepo, closeRepo } = useRepoActions({
    repos,
    setRepos,
    setRepo,
    setPane,
    setTab,
    setPickerOpen,
    dropMaximized,
    handle,
  });

  useEffect(() => {
    bumpPaneRequest();
    setCommitDrillDown(null);
    setPane({ kind: "empty" });
    resetLog();
  }, [repo, resetLog, bumpPaneRequest, setCommitDrillDown]);

  if (authed === null) return <LoadingSplash />;
  if (!authed) return <Login onSuccess={() => setAuthed(true)} />;
  if (!reposLoaded) return <LoadingSplash />;
  const current = repos.find((r) => r.id === repo);
  const q = filter.toLowerCase();
  const files = (status?.files ?? []).filter((f) =>
    f.path.toLowerCase().includes(q),
  );
  const visibleCommitFiles = (commitDrillDown?.files ?? []).filter((f) =>
    f.path.toLowerCase().includes(q) ||
    f.old_path?.toLowerCase().includes(q),
  );
  const aheadOids = new Set(
    commits.slice(0, status?.tracking?.ahead ?? 0).map((c) => c.oid),
  );

  const filesMax = maximized === "files";
  const rows = !repo
    ? "grid-rows-[auto_1fr]"
    : maximized === "terminal"
      ? "grid-rows-[auto_minmax(0,0fr)_minmax(0,1fr)_auto]"
      : maximized === "files"
        ? "grid-rows-[auto_minmax(0,1fr)_minmax(0,0fr)_auto]"
        : "grid-rows-[auto_minmax(0,11fr)_minmax(0,9fr)_auto]";

  return (
    <div className={`nc-fade grid h-full ${rows}`}>
      <Header
        repos={repos}
        repo={repo}
        setRepo={(id) => {
          setRepo(id);
          setPane({ kind: "empty" });
        }}
        setPane={() => setPane({ kind: "empty" })}
        closeRepo={closeRepo}
        setPickerOpen={setPickerOpen}
        accent={accent}
        next={next}
        cycle={cycle}
      />

      {repo ? (
        <RepoShell
          repo={repo}
          repos={repos}
          current={current}
          status={status}
          files={files}
          now={now}
          hotWindowMs={hotWindowMs}
          pane={pane}
          setPane={setPane}
          tab={tab}
          setTab={setTab}
          filter={filter}
          setFilter={setFilter}
          filterOpen={filterOpen}
          setFilterOpen={setFilterOpen}
          openDiff={openDiff}
          openFile={openFile}
          openCommit={openCommit}
          openCommitFileDiff={openCommitFileDiff}
          openCommitFiles={openCommitFiles}
          authed={authed}
          handle={handle}
          sidebarWidth={sidebarWidth}
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
          mdRendered={mdRendered}
          setMdRendered={setMdRendered}
          maximized={maximized}
          setMaximized={setMaximized}
        />
      ) : (
        <div className="flex items-center justify-center p-6 text-center text-ink-400">
          <span>
            No repository open. Click{" "}
            <span className="text-ink-200">+ open</span> above to add one.
          </span>
        </div>
      )}
      {pickerOpen && (
        <FolderPicker
          onClose={() => setPickerOpen(false)}
          onOpened={selectOpenedRepo}
        />
      )}
    </div>
  );
}
