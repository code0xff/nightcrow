import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { isUnauthorized } from "../api";
import { toast } from "../lib/toast";
import { useHotClock } from "../hooks/ui/useHotClock";
import { useShellLayout } from "../hooks/useShellLayout";
import { useProjectTabs } from "../hooks/useProjectTabs";
import { useStatus } from "../hooks/useStatus";
import { usePaneOpeners } from "../hooks/usePaneOpeners";
import { useLog } from "../hooks/useLog";
import { useResumeTick } from "../hooks/useResumeTick";
import { useRepoActions } from "../hooks/useRepoActions";
import { useClone } from "../hooks/useClone";
import { Header } from "../components/Header";
import { RepoShell } from "../components/RepoShell";
import { FolderPicker } from "../components/FolderPicker";
import { LoadingSplash } from "../components/LoadingSplash";
import { Login } from "../components/Login";
import type { Maximized, MobileView, Pane, Tab } from "../types";
import { appRows } from "../layout/appLayout";

export function App() {
  const [authed, setAuthed] = useState<boolean | null>(null);
  const [tab, setTab] = useState<Tab>("status");
  const [filter, setFilter] = useState("");
  const [filterOpen, setFilterOpen] = useState(false);
  const [pane, setPane] = useState<Pane>({ kind: "empty" });
  const [mobileView, setMobileView] = useState<MobileView>("files");
  // Prevent stale pane responses from overwriting the new context.
  const paneRequestRef = useRef(0);
  const bumpPaneRequest = useCallback(() => {
    paneRequestRef.current += 1;
  }, []);
  const [pickerOpen, setPickerOpen] = useState(false);
  const {
    accent,
    next,
    cycle,
    upperPct,
    maximizedPanelOf,
    setMaximizedFor,
    dropMaximized,
    shell,
    guards,
  } = useShellLayout();
  const [previewRendered, setPreviewRendered] = useState(true);
  const handle = useCallback((err: unknown) => {
    if (isUnauthorized(err)) {
      setAuthed(false);
      return;
    }
    toast.error(err instanceof Error ? err.message : "request failed");
  }, []);

  const resumeTick = useResumeTick();
  const {
    repos,
    setRepos,
    repo,
    setRepo,
    hot,
    clockSkewMs,
    reposLoaded,
    canClone,
    orderWrites,
    draggingRepo,
    dragOverRepo,
    onRepoDragStart,
    onRepoDragMove,
    onRepoDragEnd,
  } = useProjectTabs({
    authed,
    setAuthed,
    handle,
    resumeTick,
    ...guards,
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
  // Bound here rather than inside the hook: the state has to be owned above
  // `useProjectTabs`, which is what produces `repo` in the first place.
  const maximized = maximizedPanelOf(repo);
  const setMaximized = useCallback(
    (next: Maximized | ((prev: Maximized) => Maximized)) =>
      setMaximizedFor(repo, next),
    [setMaximizedFor, repo],
  );

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
      setMobileView,
      setPreviewRendered,
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
    orderWrites,
  });

  // Above the picker on purpose: a clone outlives the dialog that started it,
  // and outlives the page too — this reattaches to one still running.
  const { busy: cloning, start: startClone } = useClone(
    selectOpenedRepo,
    authed === true,
  );

  // Before paint: the commits, the open diff and the drill-down all belong to
  // the project being left, and a passive effect can let them show for a frame
  // under the new project's name.
  useLayoutEffect(() => {
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
  const rows = appRows(repo, maximized);

  return (
    <div
      className={`nc-fade grid h-full ${rows}`}
      style={
        {
          // Percentages as `fr` pairs so the two panels divide the space
          // between them and nothing else has to know the total.
          "--nc-upper": `${upperPct}fr`,
          "--nc-lower": `${100 - upperPct}fr`,
        } as CSSProperties
      }
    >
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
        cloning={cloning}
        accent={accent}
        next={next}
        cycle={cycle}
        draggingRepo={draggingRepo}
        dragOverRepo={dragOverRepo}
        onRepoDragStart={onRepoDragStart}
        onRepoDragMove={onRepoDragMove}
        onRepoDragEnd={onRepoDragEnd}
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
          {...shell}
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
          previewRendered={previewRendered}
          setPreviewRendered={setPreviewRendered}
          maximized={maximized}
          setMaximized={setMaximized}
          mobileView={mobileView}
          setMobileView={setMobileView}
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
          canClone={canClone}
          cloning={cloning}
          onClone={startClone}
        />
      )}
    </div>
  );
}
