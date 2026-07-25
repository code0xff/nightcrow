import { PathLabel } from "./PathLabel";
import { formatRelativeTime, statusColor } from "../utils";
import type { Commit } from "../api";
import type { CommitDrillDown } from "../hooks/useLog";

export interface LogListProps {
  visibleCommits: Commit[];
  commits: Commit[];
  aheadOids: Set<string>;
  commitDrillDown: CommitDrillDown | null;
  visibleCommitFiles: CommitDrillDown["files"];
  logDone: boolean;
  logStalled: boolean;
  logPagingPaused: boolean;
  setLogStalled: (v: boolean | ((prev: boolean) => boolean)) => void;
  logSentinelRef: React.RefObject<HTMLLIElement | null>;
  openCommitFiles: (commit: Commit) => void;
  openCommit: (oid: string) => void;
  openCommitFileDiff: (oid: string, path: string) => void;
  setCommitDrillDown: (v: CommitDrillDown | null) => void;
  setPaneEmpty: () => void;
  bumpPaneRequest: () => void;
}

export function LogList({
  visibleCommits,
  commits,
  aheadOids,
  commitDrillDown,
  visibleCommitFiles,
  logDone,
  logStalled,
  logPagingPaused,
  setLogStalled,
  logSentinelRef,
  openCommitFiles,
  openCommit,
  openCommitFileDiff,
  setCommitDrillDown,
  setPaneEmpty,
  bumpPaneRequest,
}: LogListProps) {
  return (
    <>
      {!commitDrillDown &&
        visibleCommits.map((c) => (
          <li key={c.oid}>
            <button
              onClick={() => void openCommitFiles(c)}
              title={`${c.author} · ${c.summary}`}
              className="flex w-max min-w-full items-baseline gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
            >
              {/* ↑ marks unpushed commits, like the TUI's ahead marker. */}
              <span className="w-2 shrink-0 text-added">
                {aheadOids.has(c.oid) ? "↑" : ""}
              </span>
              <span className="shrink-0 text-accent">{c.short_id}</span>
              <span className="w-10 shrink-0 text-right text-ink-400">
                {formatRelativeTime(c.time)}
              </span>
              {/* Author stays a fixed column so summaries line up, the
                  same cap the TUI applies at 10 chars; `title` carries
                  the full name. */}
              <span className="max-w-[6rem] shrink-0 truncate text-ink-400">
                {c.author}
              </span>
              <span className="whitespace-nowrap">{c.summary}</span>
            </button>
          </li>
        ))}
      {/* Asks for the next page as it scrolls into view, the way the TUI
          prefetches as the cursor nears the loaded tail. Rendered only
          while there is more, so reaching the end of the history stops
          the observer rather than leaving it to fire on every scroll.
          Kept out of the drill-down, which lists one commit's files. */}
      {!commitDrillDown && !logDone && !logStalled && !logPagingPaused && (
        <li ref={logSentinelRef} className="px-3 py-1 text-ink-400" aria-hidden="true">
          loading…
        </li>
      )}
      {/* Says why the list stops where it does while a filter is up: the
          query matches what is loaded, and more history is only a cleared
          filter away. Without this the end of a filtered list is
          indistinguishable from the end of the history. */}
      {!commitDrillDown && !logDone && !logStalled && logPagingPaused && (
        <li className="px-3 py-1 text-ink-400">
          filtering {commits.length} loaded commits — clear the filter to load
          more
        </li>
      )}
      {/* A failed page keeps its place in the list. The history did not
          end here, and the error toast fades on its own, so without this
          the list would simply look shorter than it is. */}
      {!commitDrillDown && logStalled && (
        <li className="px-3 py-1">
          <button
            onClick={() => setLogStalled(false)}
            className="text-ink-400 hover:text-accent"
          >
            could not load more — retry
          </button>
        </li>
      )}
      {commitDrillDown && (
        <>
          <li className="sticky top-0 z-10 flex w-max min-w-full items-center gap-1 bg-ink-900 px-2 py-1 text-ink-400">
            <button
              onClick={() => {
                bumpPaneRequest();
                setCommitDrillDown(null);
                setPaneEmpty();
              }}
              className="rounded-sm px-1 hover:text-accent"
              title="Back to commit log"
            >
              &lt; log
            </button>
            <span className="text-ink-600">·</span>
            <span className="shrink-0 text-accent">
              {commitDrillDown.commit.short_id}
            </span>
            <button
              onClick={() => openCommit(commitDrillDown.commit.oid)}
              className="rounded-sm px-1 hover:text-accent"
              title="Show the complete commit diff"
            >
              all changes
            </button>
          </li>
          {visibleCommitFiles.map((f) => (
            <li key={f.path}>
              <button
                onClick={() =>
                  openCommitFileDiff(commitDrillDown.commit.oid, f.path)
                }
                className="flex w-max min-w-full gap-2 px-3 py-0.5 text-left hover:bg-ink-850"
              >
                <span className={statusColor(f.index)}>{f.index}</span>
                <PathLabel path={f.path} from={f.old_path} />
              </button>
            </li>
          ))}
          {commitDrillDown.files.length === 0 && (
            <li className="px-3 py-2 text-ink-400">No changed files.</li>
          )}
          {commitDrillDown.files.length > 0 && visibleCommitFiles.length === 0 && (
            <li className="px-3 py-2 text-ink-400">No matching files.</li>
          )}
          {commitDrillDown.truncated && (
            <li className="px-3 py-1 text-accent">
              Showing the first {commitDrillDown.files.length} files.
            </li>
          )}
        </>
      )}
    </>
  );
}